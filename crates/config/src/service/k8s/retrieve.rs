use futures_util::future::join_all;
use gateway::{SgListener, SgParameters, SgProtocolConfig, SgTlsConfig};
use http_route::SgHttpRouteRule;
use k8s_gateway_api::{Gateway, HttpRoute, Listener};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, Api, ResourceExt};
use spacegate_model::{
    ext::k8s::{
        crd::{
            http_spaceroute::HttpSpaceroute,
            sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
        },
        helper_struct::SgTargetKind,
    },
    PluginInstanceId,
};

use crate::{
    constants::{self, GATEWAY_CLASS_NAME},
    model::{gateway, http_route, PluginConfig, SgGateway, SgHttpRoute},
    service::Retrieve,
    BoxError, BoxResult,
};

use super::{
    convert::{filter_k8s_conv::PluginConfigConv, gateway_k8s_conv::SgParametersConv as _, route_k8s_conv::SgHttpRouteRuleConv as _},
    K8s,
};

impl Retrieve for K8s {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> BoxResult<Option<SgGateway>> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = if let Some(gateway_obj) = gateway_api.get_opt(gateway_name).await?.and_then(|gateway_obj| {
            if gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME {
                Some(gateway_obj)
            } else {
                None
            }
        }) {
            Some(self.kube_gateway_2_sg_gateway(gateway_obj).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<Option<SgHttpRoute>> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let result = if let Some(httpspaceroute) = http_spaceroute_api.get_opt(route_name).await?.and_then(|http_route_obj| {
            if http_route_obj
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == http_route_obj.namespace() && parent_ref.name == gateway_name))
                .unwrap_or(false)
            {
                Some(http_route_obj)
            } else {
                None
            }
        }) {
            Some(self.kube_httpspaceroute_2_sg_route(httpspaceroute).await?)
        } else if let Some(http_route) = httproute_api.get_opt(route_name).await?.and_then(|http_route| {
            if http_route
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == http_route.namespace() && parent_ref.name == gateway_name))
                .unwrap_or(false)
            {
                Some(http_route)
            } else {
                None
            }
        }) {
            Some(self.kube_httproute_2_sg_route(http_route).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> BoxResult<Vec<String>> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let mut result: Vec<String> = http_spaceroute_api
            .list(&ListParams::default())
            .await?
            .iter()
            .filter(|route| {
                route
                    .spec
                    .inner
                    .parent_refs
                    .as_ref()
                    .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == route.namespace() && parent_ref.name == name))
                    .unwrap_or(false)
            })
            .map(|route| route.name_any())
            .collect();

        result.extend(
            httproute_api
                .list(&ListParams::default())
                .await?
                .iter()
                .filter(|route| {
                    route
                        .spec
                        .inner
                        .parent_refs
                        .as_ref()
                        .map(|parent_refs| parent_refs.iter().any(|parent_ref| parent_ref.namespace == route.namespace() && parent_ref.name == name))
                        .unwrap_or(false)
                })
                .map(|route| route.name_any()),
        );

        Ok(result)
    }

    async fn retrieve_config_names(&self) -> BoxResult<Vec<String>> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = gateway_api.list(&ListParams::default()).await?.iter().map(|gateway| gateway.name_any()).collect();

        Ok(result)
    }

    async fn retrieve_all_plugins(&self) -> Result<Vec<PluginConfig>, BoxError> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();

        let result = filter_api.list(&ListParams::default()).await?.into_iter().filter_map(PluginConfig::from_first_filter_obj).collect();
        Ok(result)
    }

    async fn retrieve_plugin(&self, id: &spacegate_model::PluginInstanceId) -> Result<Option<PluginConfig>, BoxError> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();

        match &id.name {
            spacegate_model::PluginInstanceName::Anon { uid: _ } => Ok(None),
            spacegate_model::PluginInstanceName::Named { name } => {
                let result = filter_api.get_opt(name).await?.and_then(PluginConfig::from_first_filter_obj);
                Ok(result)
            }
            spacegate_model::PluginInstanceName::Mono => Ok(None),
        }
    }

    async fn retrieve_plugins_by_code(&self, code: &str) -> Result<Vec<PluginConfig>, BoxError> {
        Ok(self.retrieve_all_plugins().await?.into_iter().filter(|p| p.code() == code).collect())
    }
}

impl K8s {
    async fn kube_gateway_2_sg_gateway(&self, gateway_obj: Gateway) -> BoxResult<SgGateway> {
        let gateway_name = gateway_obj.name_any();
        let plugins = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind: SgTargetKind::Gateway.into(),
                name: gateway_name.clone(),
                namespace: gateway_obj.namespace(),
            })
            .await?;
        let result = SgGateway {
            name: gateway_name,
            parameters: SgParameters::from_kube_gateway(&gateway_obj),
            listeners: self.retrieve_config_item_listeners(&gateway_obj.spec.listeners).await?,
            plugins,
        };
        Ok(result)
    }

    async fn kube_httpspaceroute_2_sg_route(&self, httpspace_route: HttpSpaceroute) -> BoxResult<SgHttpRoute> {
        let route_name = httpspace_route.name_any();
        let kind = if let Some(kind) = httpspace_route.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
            kind.clone()
        } else {
            SgTargetKind::Httpspaceroute.into()
        };
        let priority = httpspace_route.annotations().get(crate::constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<i16>().ok()).unwrap_or(0);
        let plugins = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind,
                name: route_name.clone(),
                namespace: httpspace_route.namespace(),
            })
            .await?;
        Ok(SgHttpRoute {
            hostnames: httpspace_route.spec.hostnames.clone(),
            plugins,
            rules: httpspace_route
                .spec
                .rules
                .map(|r_vec| r_vec.into_iter().map(SgHttpRouteRule::from_kube_httproute).collect::<Result<Vec<_>, BoxError>>())
                .transpose()?
                .unwrap_or_default(),
            priority,
            route_name,
        })
    }

    async fn kube_httproute_2_sg_route(&self, http_route: HttpRoute) -> BoxResult<SgHttpRoute> {
        self.kube_httpspaceroute_2_sg_route(http_route.into()).await
    }

    async fn retrieve_config_item_filters(&self, target: K8sSgFilterSpecTargetRef) -> BoxResult<Vec<PluginInstanceId>> {
        let kind = target.kind;
        let name = target.name;
        let namespace = target.namespace.unwrap_or(self.namespace.to_string());

        let filter_api: Api<SgFilter> = self.get_all_api();
        let plugin_ids: Vec<PluginInstanceId> = filter_api
            .list(&ListParams::default())
            .await
            .map_err(Box::new)?
            .into_iter()
            .filter(|filter_obj| {
                filter_obj.spec.target_refs.iter().any(|target_ref| {
                    target_ref.kind.eq_ignore_ascii_case(&kind)
                        && target_ref.name.eq_ignore_ascii_case(&name)
                        && target_ref.namespace.as_deref().unwrap_or("default").eq_ignore_ascii_case(&namespace)
                })
            })
            .flat_map(|filter_obj| PluginConfig::from_first_filter_obj(filter_obj).map(|f| f.into()))
            .collect();

        if !plugin_ids.is_empty() {
            let mut filter_vec = String::new();
            plugin_ids.clone().into_iter().for_each(|id| filter_vec.push_str(&format!("plugin:{{id:{}}},", id.to_string())));
            tracing::trace!("[SG.Common] {namespace}.{kind}.{name} filter found: {}", filter_vec.trim_end_matches(','));
        }

        if plugin_ids.is_empty() {
            Ok(vec![])
        } else {
            Ok(plugin_ids)
        }
    }

    async fn retrieve_config_item_listeners(&self, listeners: &[Listener]) -> BoxResult<Vec<SgListener>> {
        join_all(
            listeners
                .iter()
                .map(|listener| async move {
                    let sg_listener = SgListener {
                        name: listener.name.clone(),
                        ip: None,
                        port: listener.port,
                        protocol: match listener.protocol.to_lowercase().as_str() {
                            "http" => SgProtocolConfig::Http,
                            "https" => {
                                if let Some(tls_config) = &listener.tls {
                                    if let Some(certificate_ref) = tls_config.certificate_refs.as_ref().and_then(|vec| vec.first()) {
                                        let secret_api: Api<Secret> = self.get_namespace_api();
                                        if let Some(secret_obj) = secret_api.get_opt(&certificate_ref.name).await? {
                                            let tls = if let Some(secret_data) = secret_obj.data {
                                                if let Some(tls_crt) = secret_data.get("tls.crt") {
                                                    if let Some(tls_key) = secret_data.get("tls.key") {
                                                        Some(SgTlsConfig {
                                                            mode: tls_config.mode.clone().into(),
                                                            key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                                                            cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
                                                        })
                                                    } else {
                                                        tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.key is empty");
                                                        None
                                                    }
                                                } else {
                                                    tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                                    None
                                                }
                                            } else {
                                                tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.data is empty");
                                                None
                                            };
                                            if let Some(tls) = tls {
                                                SgProtocolConfig::Https { tls }
                                            } else {
                                                SgProtocolConfig::Http
                                            }
                                        } else {
                                            tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                            SgProtocolConfig::Http
                                        }
                                    } else {
                                        tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                        SgProtocolConfig::Http
                                    }
                                } else {
                                    tracing::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls is empty");
                                    SgProtocolConfig::Http
                                }
                            }
                            _ => return Err("Unsupported protocol".into()),
                        },
                        hostname: listener.hostname.clone(),
                    };
                    Ok(sg_listener)
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect()
    }
}
