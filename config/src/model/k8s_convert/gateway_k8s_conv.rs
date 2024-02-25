use std::collections::BTreeMap;

use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference};
use k8s_openapi::api::core::v1::Secret;
use kube::api::ObjectMeta;

use crate::{constants, k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef, SgFilterTargetKind}, model::{helper_filter::SgSingeFilter, SgGateway, SgParameters}};


impl SgGateway {
    pub fn to_kube_gateway(self,namespace: &str) -> (Gateway,Option<Secret>, Vec<SgSingeFilter>) {
        let mut secret =None;

        let gateway = Gateway {
            metadata: ObjectMeta {
                annotations: Some(self.parameters.into_kube_gateway()),
                labels: None,
                name: Some(self.name),
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
                        tls: match l.protocol{
                            crate::model::SgProtocolConfig::Http => None,
                            crate::model::SgProtocolConfig::Https { tls } => match tls {
                                crate::model::SgTlsConfig::K8sService { k8sconfig, mode,} => Some(GatewayTlsConfig {
                                    mode: Some(tls.mode.to_kube_tls_mode_type()),
                                    certificate_refs: Some(vec![SecretObjectReference {
                                        kind: Some("Secret".to_string()),
                                        name:k8sconfig.name,
                                        namespace: k8sconfig.namespace,
                                        ..Default::default()
                                    }]),
                                    options: None,
                                }),
                                crate::model::SgTlsConfig::Other { mode, key, cert } => {
                                    let name=todo!();
                                    secret = Some(Secret {
                                        metadata: ObjectMeta {
                                            name: Some(name),
                                            namespace: Some(namespace.to_string()),
                                            ..Default::default()
                                        },
                                        type_: Some("kubernetes.io/tls".to_string()),
                                        data: Some(BTreeMap::from([
                                            ("tls.key".to_string(), key),
                                            ("tls.crt".to_string(), cert),
                                        ])),
                                        ..Default::default()
                                    });
                                    Some(GatewayTlsConfig {
                                        mode: Some(mode.to_kube_tls_mode_type()),
                                        certificate_refs: Some(vec![SecretObjectReference {
                                            kind: Some("Secret".to_string()),
                                            name,
                                            namespace: namespace.to_string(),
                                            ..Default::default()
                                        }]),
                                        options: None,
                                    })
                                },
                            },
                        },
                        allowed_routes: None,
                    })
                    .collect(),
                addresses: None,
            },
            status: None,
        };

        let sgfilters: Vec<SgSingeFilter> = 
            self.filters
                .into_iter()
                .map(|f| SgSingeFilter {
                    name: f.name,
                    namespace: namespace.to_string(),
                    filter: K8sSgFilterSpecFilter {
                        code: f.code,
                        name: None,
                        enable: true,
                        config: f.spec,
                    },
                    target_ref: K8sSgFilterSpecTargetRef {
                        kind: SgFilterTargetKind::Gateway.into(),
                        name: self.name.clone(),
                        namespace: Some(namespace.to_string()),
                    },
                })
                .collect();
        

        (gateway,secret, sgfilters)
    }
}

impl SgParameters {
    pub(crate) fn into_kube_gateway(self) -> BTreeMap<String, String> {
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

    pub(crate) fn from_kube_gateway(gateway: &Gateway) -> Self {
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