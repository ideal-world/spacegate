use std::collections::BTreeMap;

use chrono::Utc;
use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference};
use k8s_openapi::{api::core::v1::Secret, ByteString};
use kube::{api::ObjectMeta, ResourceExt};
use spacegate_model::{ext::k8s::helper_struct::SgTargetKind, PluginInstanceId};

use crate::{constants, ext::k8s::crd::sg_filter::K8sSgFilterSpecTargetRef, SgGateway, SgParameters};

use super::ToTarget;
pub(crate) trait SgGatewayConv {
    fn to_kube_gateway(self, namespace: &str) -> (Gateway, Option<Secret>, Vec<PluginInstanceId>);
}
impl SgGatewayConv for SgGateway {
    fn to_kube_gateway(self, namespace: &str) -> (Gateway, Option<Secret>, Vec<PluginInstanceId>) {
        let mut secret = None;

        let gateway = Gateway {
            metadata: ObjectMeta {
                annotations: Some(self.parameters.into_kube_gateway()),
                labels: None,
                name: Some(self.name.clone()),
                owner_references: None,
                self_link: None,
                ..Default::default()
            },
            spec: GatewaySpec {
                gateway_class_name: constants::GATEWAY_CLASS_NAME.to_string(),
                listeners: self
                    .listeners
                    .into_iter()
                    .map(|l| Listener {
                        name: l.name,
                        hostname: l.hostname,
                        port: l.port,
                        protocol: l.protocol.to_string(),
                        tls: match l.protocol {
                            crate::SgProtocolConfig::Http => None,
                            crate::SgProtocolConfig::Https { tls } => {
                                let current_time_utc = Utc::now().timestamp();
                                let name = tls.key[..2].to_string() + &current_time_utc.to_string();
                                secret = Some(Secret {
                                    metadata: ObjectMeta {
                                        name: Some(name.clone()),
                                        namespace: Some(namespace.to_string()),
                                        ..Default::default()
                                    },
                                    type_: Some("kubernetes.io/tls".to_string()),
                                    data: Some(BTreeMap::from([
                                        ("tls.key".to_string(), ByteString(tls.key.into_bytes())),
                                        ("tls.crt".to_string(), ByteString(tls.cert.into_bytes())),
                                    ])),
                                    ..Default::default()
                                });
                                Some(GatewayTlsConfig {
                                    mode: Some(tls.mode.into()),
                                    certificate_refs: Some(vec![SecretObjectReference {
                                        kind: Some("Secret".to_string()),
                                        name,
                                        namespace: Some(namespace.to_string()),
                                        ..Default::default()
                                    }]),
                                    options: None,
                                })
                            }
                            _ => None,
                        },
                        allowed_routes: None,
                    })
                    .collect(),
                addresses: None,
            },
            status: None,
        };

        (gateway, secret, self.plugins)
    }
}

pub(crate) trait SgParametersConv {
    fn from_kube_gateway(gateway: &Gateway) -> Self;
    fn into_kube_gateway(self) -> BTreeMap<String, String>;
}

impl SgParametersConv for SgParameters {
    fn into_kube_gateway(self) -> BTreeMap<String, String> {
        let mut ann = BTreeMap::new();
        if let Some(redis_url) = self.redis_url {
            ann.insert(crate::constants::GATEWAY_ANNOTATION_REDIS_URL.to_string(), redis_url);
        }
        if let Some(log_level) = self.log_level {
            ann.insert(crate::constants::GATEWAY_ANNOTATION_LOG_LEVEL.to_string(), log_level);
        }
        if let Some(lang) = self.lang {
            ann.insert(crate::constants::GATEWAY_ANNOTATION_LANGUAGE.to_string(), lang);
        }
        if let Some(ignore_tls_verification) = self.ignore_tls_verification {
            ann.insert(
                crate::constants::GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION.to_string(),
                ignore_tls_verification.to_string(),
            );
        }
        ann
    }

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

impl ToTarget for Gateway {
    fn to_target_ref(&self) -> K8sSgFilterSpecTargetRef {
        K8sSgFilterSpecTargetRef {
            kind: SgTargetKind::Gateway.into(),
            name: self.name_any(),
            namespace: self.namespace(),
        }
    }
}
