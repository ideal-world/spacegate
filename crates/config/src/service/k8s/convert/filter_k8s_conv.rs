use k8s_gateway_api::{HttpPathModifier, HttpRouteFilter, LocalObjectReference};
use kube::{
    api::{DeleteParams, PostParams},
    Api, ResourceExt,
};
use spacegate_model::{constants::SG_FILTER_KIND, ext::k8s::crd::sg_filter::SgFilter};

use crate::{
    ext::k8s::{
        crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef},
        helper_struct::SgSingeFilter,
    },
    plugin::{
        gatewayapi_support_filter::{SgHttpPathModifier, SgHttpPathModifierType},
        PluginConfig,
    },
    service::k8s::K8s,
    BoxResult, PluginInstanceId, PluginInstanceName,
};

pub(crate) trait PluginIdConv {
    /// # to_singe_filter
    /// `to_single_filter` method and [SgRouteFilter::to_http_route_filter] method both convert from
    /// `SgRouteFilter`to the k8s model. The difference lies in that `to_single_filter` only includes
    /// the k8s object of `SGFilter`, whereas `to_http_route_filter` is used to convert to `HttpRouteFilter`,
    /// a filter defined in the Gateway API.
    fn to_singe_filter(&self, value: serde_json::Value, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter>;

    /// # to_http_route_filter
    /// ref [SgRouteFilter::to_singe_filter]
    /// can be use in rule level and backend level
    fn to_http_route_filter(self) -> Option<HttpRouteFilter>;

    fn from_http_route_filter(route_filter: HttpRouteFilter) -> Option<PluginInstanceId>;

    /// can be ues in gateway and route level
    async fn add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()>;

    /// can be ues in gateway and route level
    async fn remove_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()>;

    // mix of [SgRouteFilter::to_singe_filter] and [SgRouteFilter::to_http_route_filter]
    // PluginInstanceId can be converted into `SgRouteFilter` or `HttpRouteFilter`
    // async fn to_route_filter_or_add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> Option<HttpRouteFilter>;
}

impl PluginIdConv for PluginInstanceId {
    fn to_singe_filter(&self, value: serde_json::Value, target: Option<K8sSgFilterSpecTargetRef>, namespace: &str) -> Option<SgSingeFilter> {
        match self.name.clone() {
            PluginInstanceName::Anon { uid: _ } => None,
            PluginInstanceName::Named { name } => Some(SgSingeFilter {
                name: name.clone(),
                namespace: namespace.to_owned(),
                filter: K8sSgFilterSpecFilter {
                    code: self.code.to_string(),
                    name: Some(name),
                    config: value,
                    enable: true,
                },
                target_ref: target,
            }),
            PluginInstanceName::Mono => None,
        }
    }

    // fn from_http_route_filter(route_filter: HttpRouteFilter) -> BoxResult<PluginConfig> {
    //     let process_header_modifier = |header_modifier: HttpRequestHeaderFilter, modifier_kind: SgFilterHeaderModifierKind| -> BoxResult<PluginConfig> {
    //         let mut sg_sets = HashMap::new();
    //         if let Some(adds) = header_modifier.add {
    //             for add in adds {
    //                 sg_sets.insert(add.name, add.value);
    //             }
    //         }
    //         if let Some(sets) = header_modifier.set {
    //             for set in sets {
    //                 sg_sets.insert(set.name, set.value);
    //             }
    //         }

    //         Ok(PluginConfig {
    //             id: PluginInstanceId {
    //                 code: SG_FILTER_HEADER_MODIFIER_CODE.into(),
    //                 name: PluginInstanceName::Mono {},
    //             },
    //             spec: serde_json::to_value(SgFilterHeaderModifier {
    //                 kind: modifier_kind,
    //                 sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
    //                 remove: header_modifier.remove,
    //             })?,
    //         })
    //     };
    //     let sg_filter = match route_filter {
    //         k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
    //             process_header_modifier(request_header_modifier, SgFilterHeaderModifierKind::Request)?
    //         }
    //         k8s_gateway_api::HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => {
    //             process_header_modifier(response_header_modifier, SgFilterHeaderModifierKind::Response)?
    //         }
    //         k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => PluginConfig {
    //             id: PluginInstanceId {
    //                 code: SG_FILTER_REDIRECT_CODE.into(),
    //                 name: PluginInstanceName::Mono {},
    //             },
    //             spec: serde_json::to_value(SgFilterRedirect {
    //                 scheme: request_redirect.scheme,
    //                 hostname: request_redirect.hostname,
    //                 path: request_redirect.path.map(|path| match path {
    //                     k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
    //                         kind: SgHttpPathModifierType::ReplaceFullPath,
    //                         value: replace_full_path,
    //                     },
    //                     k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
    //                         kind: SgHttpPathModifierType::ReplacePrefixMatch,
    //                         value: replace_prefix_match,
    //                     },
    //                 }),
    //                 port: request_redirect.port,
    //                 status_code: request_redirect.status_code,
    //             })?,
    //         },
    //         k8s_gateway_api::HttpRouteFilter::URLRewrite { url_rewrite } => PluginConfig {
    //             id: PluginInstanceId {
    //                 code: SG_FILTER_REWRITE_CODE.into(),
    //                 name: PluginInstanceName::Mono {},
    //             },
    //             spec: serde_json::to_value(SgFilterRewrite {
    //                 hostname: url_rewrite.hostname,
    //                 path: url_rewrite.path.map(|path| match path {
    //                     k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
    //                         kind: SgHttpPathModifierType::ReplaceFullPath,
    //                         value: replace_full_path,
    //                     },
    //                     k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
    //                         kind: SgHttpPathModifierType::ReplacePrefixMatch,
    //                         value: replace_prefix_match,
    //                     },
    //                 }),
    //             })?,
    //         },
    //         k8s_gateway_api::HttpRouteFilter::RequestMirror { .. } => return Err("[SG.Common] HttpRoute [spec.rules.filters.type=RequestMirror] not supported yet".into()),
    //         k8s_gateway_api::HttpRouteFilter::ExtensionRef { .. } => return Err("[SG.Common] HttpRoute [spec.rules.filters.type=ExtensionRef] not supported yet".into()),
    //     };
    //     Ok(sg_filter)
    // }

    async fn add_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = client.get_namespace_api();
        if let Ok(mut filter) = filter_api.get(&self.name.to_string()).await {
            if !filter.spec.target_refs.iter().any(|t| t.eq(&target)) {
                filter.spec.target_refs.push(target);
                filter_api.replace(&filter.name_any(), &PostParams::default(), &filter).await?;
            };
        }
        Ok(())
    }

    async fn remove_filter_target(&self, target: K8sSgFilterSpecTargetRef, client: &K8s) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = client.get_namespace_api();
        if let Ok(mut filter) = filter_api.get(&self.name.to_string()).await {
            if filter.spec.target_refs.iter().any(|t| t.eq(&target)) {
                filter.spec.target_refs.retain(|t| !t.eq(&target));

                if filter.spec.target_refs.is_empty() {
                    filter_api.delete(&filter.name_any(), &DeleteParams::default()).await?;
                } else {
                    filter_api.replace(&filter.name_any(), &PostParams::default(), &filter).await?;
                }
            };
        }
        Ok(())
    }

    fn to_http_route_filter(self) -> Option<HttpRouteFilter> {
        match self.name {
            PluginInstanceName::Anon { uid: _ } => None,
            PluginInstanceName::Named { name } => Some(HttpRouteFilter::ExtensionRef {
                extension_ref: LocalObjectReference {
                    group: "".to_string(),
                    kind: SG_FILTER_KIND.to_string(),
                    name,
                },
            }),
            PluginInstanceName::Mono => None,
        }
    }

    fn from_http_route_filter(route_filter: HttpRouteFilter) -> Option<PluginInstanceId> {
        match route_filter {
            HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => None,
            HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => None,
            HttpRouteFilter::RequestMirror { request_mirror } => None,
            HttpRouteFilter::RequestRedirect { request_redirect } => None,
            HttpRouteFilter::URLRewrite { url_rewrite } => None,
            HttpRouteFilter::ExtensionRef { extension_ref } => Some(PluginInstanceId {
                code: extension_ref.kind.into(),
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
            id: PluginInstanceId {
                code: f.code.into(),
                name: PluginInstanceName::Named {
                    name: f.name.unwrap_or(filter_name.clone()),
                },
            },
            spec: f.config,
        })
    }
}

pub(crate) trait PathModifierConv {
    fn to_http_path_modifier(self) -> HttpPathModifier;
}

impl PathModifierConv for SgHttpPathModifier {
    fn to_http_path_modifier(self) -> HttpPathModifier {
        match self.kind {
            SgHttpPathModifierType::ReplaceFullPath => HttpPathModifier::ReplaceFullPath { replace_full_path: self.value },
            SgHttpPathModifierType::ReplacePrefixMatch => HttpPathModifier::ReplacePrefixMatch { replace_prefix_match: self.value },
        }
    }
}
