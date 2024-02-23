use gateway::{SgListener, SgParameters, SgProtocolConfig, SgTlsConfig, SgTlsMode};
use k8s_gateway_api::{Gateway, Listener};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, Api, ResourceExt};
use tardis::{futures::future::join_all, log};

use super::Retrieve;
use crate::{
    backend::k8s::K8s,
    constants::GATEWAY_CLASS_NAME,
    k8s_crd::sg_filter::{K8sSgFilterSpecTargetRef, SgFilter, SgFilterTargetKind},
    model::{gateway, SgGateway, SgHttpRoute, SgRouteFilter},
};

impl Retrieve for K8s {
    type Error = kube::Error;

    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<SgGateway>, Self::Error> {
        let gateway_api: Api<Gateway> = self.get_all_api();

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

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<SgHttpRoute>, Self::Error> {
        todo!()
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, Self::Error> {
        todo!()
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, Self::Error> {
        todo!()
    }
}

impl K8s {
    async fn from_kube_gateway(&self, gateway_obj: Gateway) -> Result<SgGateway, <Self as Retrieve>::Error> {
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
    async fn retrieve_config_item_filters(&self, target: K8sSgFilterSpecTargetRef) -> Result<Vec<SgRouteFilter>, <Self as Retrieve>::Error> {}
    async fn retrieve_config_item_listeners(&self, listeners: &Vec<Listener>) -> Result<Vec<SgListener>, <Self as Retrieve>::Error> {
        join_all(
            listeners
                .into_iter()
                .map(|listener| async move {
                    let sg_listener = SgListener {
                        name: Some(listener.name),
                        ip: None,
                        port: listener.port,
                        protocol: match listener.protocol.to_lowercase().as_str() {
                            "http" => SgProtocolConfig::Http,
                            "https" => {
                                if let Some(tls_config) = listener.tls {
                                    if let Some(certificate_ref) = tls_config.certificate_refs.as_ref().and_then(|vec| vec.get(0)) {
                                        let secret_api: Api<Secret> = self.get_namespace_api();
                                        if let Some(secret_obj) = secret_api.get_opt(&certificate_ref.name).await? {
                                            let tls = if let Some(secret_data) = secret_obj.data {
                                                if let Some(tls_crt) = secret_data.get("tls.crt") {
                                                    if let Some(tls_key) = secret_data.get("tls.key") {
                                                        Some(SgTlsConfig {
                                                            mode: SgTlsMode::from(tls_config.mode).unwrap_or_default(),
                                                            key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                                                            cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
                                                        })
                                                    } else {
                                                        log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.key is empty");
                                                        None
                                                    }
                                                } else {
                                                    log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                                    None
                                                }
                                            } else {
                                                log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.data is empty");
                                                None
                                            };
                                            if let Some(tls) = tls {
                                                SgProtocolConfig::Https { tls }
                                            } else {
                                                SgProtocolConfig::Http
                                            }
                                        } else {
                                            log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                            SgProtocolConfig::Http
                                        }
                                    } else {
                                        log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls.certificate_refs is empty");
                                        SgProtocolConfig::Http
                                    }
                                } else {
                                    log::warn!("[SG.Config] Gateway [spec.listener.protocol=https] tls is empty");
                                    SgProtocolConfig::Http
                                }
                            }
                        },
                        hostname: listener.hostname,
                    };
                    sg_listener
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

impl SgTlsConfig {}
