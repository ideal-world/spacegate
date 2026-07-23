use k8s_gateway_api::{HttpRouteFilter, LocalObjectReference};
use kube::{api::PostParams, Api, ResourceExt};
use spacegate_model::{constants::SG_FILTER_KIND, ext::k8s::crd::sg_filter::SgFilter};

use crate::{
    ext::k8s::{
        crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef},
        helper_struct::SgSingeFilter,
    },
    plugin::PluginConfig,
    service::k8s::K8s,
    BoxResult, PluginInstanceId, PluginInstanceName,
};

pub(crate) trait PluginIdConv {
    /// # to_singe_filter
    /// `to_single_filter` method and [SgRouteFilter::to_http_route_filter] method both convert from
    /// `SgRouteFilter`to the k8s model. The difference lies in that `to_single_filter` only includes
    /// the k8s object of `SGFilter`, whereas `to_http_route_filter` is used to convert to `HttpRouteFilter`,
    /// a filter defined in the Gateway API.
    fn to_singe_filter(&self, value: serde_json::Value, display_name: Option<String>, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter>;

    /// # to_http_route_filter
    /// ref [SgRouteFilter::to_singe_filter]
    /// can be use in rule level and backend level
    fn to_http_route_filter(self) -> Option<HttpRouteFilter>;

    fn from_http_route_filter(route_filter: HttpRouteFilter) -> Option<PluginInstanceId>;

    /// can be ues in gateway and route level
    async fn add_filter_target(&self, target: K8sSgFilterSpecTargetRef, priority: i32, client: &K8s) -> BoxResult<()>;

    /// can be ues in gateway and route level
    async fn remove_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()>;

    fn extract_from_filter(filter: &K8sSgFilterSpecFilter, default_name: &str) -> PluginInstanceId {
        let code = filter.code.clone().into();
        let name = filter.name.clone().unwrap_or(default_name.to_string());
        PluginInstanceId {
            code,
            name: PluginInstanceName::Named { name },
        }
    }
}

impl PluginIdConv for PluginInstanceId {
    fn to_singe_filter(&self, value: serde_json::Value, display_name: Option<String>, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter> {
        match self.name.clone() {
            PluginInstanceName::Anon { uid: _ } => None,
            PluginInstanceName::Named { name } => Some(SgSingeFilter {
                name: name.clone(),
                namespace: namespace.to_owned(),
                filter: K8sSgFilterSpecFilter {
                    code: self.code.to_string(),
                    name: Some(name),
                    display_name: crate::service::normalize_plugin_display_name(display_name),
                    config: value,
                    enable: true,
                },
                target_ref: target,
            }),
            PluginInstanceName::Mono => None,
        }
    }

    async fn add_filter_target(&self, mut target: K8sSgFilterSpecTargetRef, priority: i32, client: &K8s) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = client.get_namespace_api();
        if let Ok(mut filter) = filter_api.get(&self.name.to_raw_str()).await {
            target.priority = priority;
            if let Some(existing) = filter.spec.target_refs.iter_mut().find(|existing| (**existing).eq(&target)) {
                if existing.priority != priority {
                    existing.priority = priority;
                    filter_api.replace(&filter.name_any(), &PostParams::default(), &filter).await?;
                }
            } else {
                filter.spec.target_refs.push(target);
                filter_api.replace(&filter.name_any(), &PostParams::default(), &filter).await?;
            }
        }
        Ok(())
    }

    async fn remove_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = client.get_namespace_api();
        if let Ok(mut filter) = filter_api.get(&self.name.to_raw_str()).await {
            if filter.spec.target_refs.iter().any(|t| t.eq(&target)) {
                filter.spec.target_refs.retain(|t| !t.eq(&target));

                filter_api.replace(&filter.name_any(), &PostParams::default(), &filter).await?;
            };
        }
        Ok(())
    }

    fn to_http_route_filter(self) -> Option<HttpRouteFilter> {
        match self.name {
            PluginInstanceName::Anon { uid: _ } => None,
            PluginInstanceName::Named { name } => Some(HttpRouteFilter::ExtensionRef {
                extension_ref: LocalObjectReference {
                    group: self.code.into(),
                    kind: SG_FILTER_KIND.to_string(),
                    name,
                },
            }),
            PluginInstanceName::Mono => None,
        }
    }

    fn from_http_route_filter(route_filter: HttpRouteFilter) -> Option<PluginInstanceId> {
        match route_filter {
            HttpRouteFilter::RequestHeaderModifier { request_header_modifier: _ } => None,
            HttpRouteFilter::ResponseHeaderModifier { response_header_modifier: _ } => None,
            HttpRouteFilter::RequestMirror { request_mirror: _ } => None,
            HttpRouteFilter::RequestRedirect { request_redirect: _ } => None,
            HttpRouteFilter::URLRewrite { url_rewrite: _ } => None,
            HttpRouteFilter::ExtensionRef { extension_ref } => Some(PluginInstanceId {
                code: extension_ref.group.into(),
                name: PluginInstanceName::Named { name: extension_ref.name },
            }),
        }
    }
}

pub(crate) trait PluginConfigConv {
    fn from_first_filter_obj(filter_obj: SgFilter) -> Option<PluginConfig>;
}

impl PluginConfigConv for PluginConfig {
    fn from_first_filter_obj(filter_obj: SgFilter) -> Option<PluginConfig> {
        let filter_name = filter_obj.name_any();
        filter_obj.spec.filters.into_iter().find(|f| f.enable).map(|f| PluginConfig {
            id: PluginInstanceId::extract_from_filter(&f, &filter_name),
            display_name: crate::service::normalize_plugin_display_name(f.display_name),
            spec: f.config,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn plugin_display_name_round_trips_through_sg_filter_without_entering_config() {
        let id = PluginInstanceId::new("hai-auth", PluginInstanceName::named("auth-a1"));
        let single = id.to_singe_filter(json!({ "cache_url": "redis://redis:6379" }), Some("生产鉴权".to_string()), None, "default").unwrap();
        let filter: SgFilter = single.into();

        assert_eq!(filter.spec.filters[0].display_name.as_deref(), Some("生产鉴权"));

        let config = PluginConfig::from_first_filter_obj(filter).unwrap();
        assert_eq!(config.display_name.as_deref(), Some("生产鉴权"));
        assert_eq!(config.spec, json!({ "cache_url": "redis://redis:6379" }));
    }
}
