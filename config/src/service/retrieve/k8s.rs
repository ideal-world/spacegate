use futures_util::future::join_all;
use gateway::{SgListener, SgParameters, SgProtocolConfig, SgTlsConfig, SgTlsMode};
use k8s_gateway_api::{Gateway, HttpRoute, Listener};
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, ResourceExt};

use super::Retrieve;
use crate::{
    constants::GATEWAY_CLASS_NAME,
    k8s_crd::{
        http_spaceroute::HttpSpaceroute,
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter, SgFilterTargetKind},
    },
    model::{gateway, http_route, SgGateway, SgHttpRoute, SgRouteFilter},
    service::backend::k8s::K8s,
    BoxError,
};

impl Retrieve for K8s {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<SgGateway>, BoxError> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        let result = if let Some(gateway_obj) = gateway_api.get_opt(&gateway_name).await?.and_then(|gateway_obj| {
            if gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME {
                Some(gateway_obj)
            } else {
                None
            }
        }) {
            Some(self.from_kube_gateway(gateway_obj).await?)
        } else {
            None
        };

        Ok(result)
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<SgHttpRoute>, BoxError> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        let result = if let Some(httpspaceroute) = http_spaceroute_api.get_opt(&route_name).await?.and_then(|http_route_obj| {
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
            Some(self.from_kube_httpspaceroute(httpspaceroute).await?)
        } else {
            if let Some(http_route) = httproute_api.get_opt(&route_name).await?.and_then(|http_route| {
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
                Some(self.from_kube_httproute(http_route).await?)
            } else {
                None
            }
        };

        Ok(result)
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, BoxError> {
        todo!()
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, BoxError> {
        todo!()
    }
}

impl K8s {
    async fn from_kube_gateway(&self, gateway_obj: Gateway) -> Result<SgGateway, BoxError> {
        let gateway_name = gateway_obj.name_any();
        let filters = self
            .retrieve_config_item_filters(K8sSgFilterSpecTargetRef {
                kind: SgFilterTargetKind::Gateway.into(),
                name: gateway_name.clone(),
                namespace: gateway_obj.namespace(),
            })
            .await?;
        let result = SgGateway {
            name: gateway_name,
            parameters: SgParameters::from_kube_gateway(&gateway_obj),
            listeners: self.retrieve_config_item_listeners(&gateway_obj.spec.listeners).await?,
            filters,
        };
        Ok(result)
    }

    async fn from_kube_httpspaceroute(&self, route: HttpSpaceroute) -> Result<SgHttpRoute, BoxError> {
        todo!()
        // let kind = if let Some(kind) = httproute.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
        //     kind
        // } else {
        //     constants::RAW_HTTP_ROUTE_KIND_SPACEROUTE
        // };
        // let priority = httproute.annotations().get(crate::constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<i64>().ok()).unwrap_or(0);
        // let gateway_refs = httproute.spec.inner.parent_refs.clone().unwrap_or_default();
        // Ok(SgHttpRoute {
        //     name: get_k8s_obj_unique(&httproute),
        //     gateway_name: format_k8s_obj_unique(
        //         gateway_refs.get(0).and_then(|x| x.namespace.clone()).as_ref(),
        //         &gateway_refs.get(0).map(|x| x.name.clone()).unwrap_or_default(),
        //     ),
        //     hostnames: httproute.spec.hostnames.clone(),
        //     filters: SgRouteFilter::from_crd_filters(client_name, kind, &httproute.metadata.name, &httproute.metadata.namespace).await?,
        //     rules: httproute.spec.rules.map(|r_vec| r_vec.into_iter().map(SgHttpRouteRule::from_kube_httproute).collect::<TardisResult<Vec<_>>>()).transpose()?,
        //     priority,
        // })
    }

    async fn from_kube_httproute(&self, route: HttpRoute) -> Result<SgHttpRoute, BoxError> {
        todo!()
        // self.from_kube_httpspaceroute().await
    }

    async fn retrieve_config_item_filters(&self, target: K8sSgFilterSpecTargetRef) -> Result<Vec<SgRouteFilter>, BoxError> {
        todo!()
    }

    async fn retrieve_config_item_listeners(&self, listeners: &Vec<Listener>) -> Result<Vec<SgListener>, BoxError> {
        join_all(
            listeners
                .into_iter()
                .map(|listener| async move {
                    let sg_listener = SgListener {
                        name: Some(listener.name.clone()),
                        ip: None,
                        port: listener.port,
                        protocol: match listener.protocol.to_lowercase().as_str() {
                            "http" => SgProtocolConfig::Http,
                            "https" => {
                                if let Some(tls_config) = &listener.tls {
                                    if let Some(certificate_ref) = tls_config.certificate_refs.as_ref().and_then(|vec| vec.get(0)) {
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

impl SgParameters {
    fn from_kube_gateway(gateway: &Gateway) -> Self {
        let gateway_annotations = gateway.metadata.annotations.clone();
        if let Some(gateway_annotations) = gateway_annotations {
            SgParameters {
                redis_url: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_REDIS_URL).map(|v| v.to_string()),
                log_level: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_LOG_LEVEL).map(|v| v.to_string()),
                lang: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_LANGUAGE).map(|v| v.to_string()),
                ignore_tls_verification: gateway_annotations.get(crate::constants::GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION).and_then(|v| v.parse::<bool>().ok()),
            }
        } else {
            SgParameters {
                redis_url: None,
                log_level: None,
                lang: None,
                ignore_tls_verification: None,
            }
        }
    }
}
