use crate::constants::GATEWAY_CLASS_NAME;
use crate::converter::plugin_k8s_conv::SgSingeFilter;
use crate::inner_model::gateway::{SgGateway, SgParameters, SgTlsConfig, SgTlsMode};
use crate::k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef};
use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference, TlsModeType};
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::ByteString;
use std::collections::BTreeMap;
use tardis::TardisFuns;

impl SgGateway {
    pub fn to_kube_gateway(self, namespace: &str) -> (Gateway, Vec<Secret>, Vec<SgSingeFilter>) {
        let mut secrets: Vec<Secret> = vec![];

        let gateway = Gateway {
            metadata: ObjectMeta {
                annotations: Some(self.parameters.to_kube_gateway()),
                labels: None,
                name: Some(self.name.clone()),
                owner_references: None,
                self_link: None,
                ..Default::default()
            },
            spec: GatewaySpec {
                gateway_class_name: GATEWAY_CLASS_NAME.to_string(),
                listeners: self
                    .listeners
                    .into_iter()
                    .map(|l| Listener {
                        name: l.name,
                        hostname: l.hostname,
                        port: l.port,
                        protocol: l.protocol.to_string(),
                        tls: l.tls.map(|l| {
                            let (tls_config, secret) = l.to_kube_tls(namespace);
                            secrets.push(secret);
                            tls_config
                        }),
                        allowed_routes: None,
                    })
                    .collect(),
                addresses: None,
            },
            status: None,
        };

        let sgfilters: Vec<SgSingeFilter> = if let Some(filters) = self.filters {
            filters
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
                        kind: "Gateway".to_string(),
                        name: self.name.clone(),
                        namespace: Some(namespace.to_string()),
                    },
                })
                .collect()
        } else {
            vec![]
        };

        (gateway, secrets, sgfilters)
    }
}

impl SgParameters {
    pub fn from_kube_gateway(gateway: &Gateway) -> Self {
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

    pub(crate) fn to_kube_gateway(self) -> BTreeMap<String, String> {
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
}

impl SgTlsConfig {
    pub fn to_kube_tls(self, namespace: &str) -> (GatewayTlsConfig, Secret) {
        let tls_name = TardisFuns::field.nanoid_custom(
            10,
            &[
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3', '4', '5',
                '6', '7', '8', '9',
            ],
        );
        (
            GatewayTlsConfig {
                mode: Some(self.mode.to_kube_tls_mode_type()),
                certificate_refs: Some(vec![SecretObjectReference {
                    kind: Some("Secret".to_string()),
                    name: tls_name.clone(),
                    namespace: Some(namespace.to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            Secret {
                data: Some(BTreeMap::from([
                    ("tls.key".to_string(), ByteString(self.key.as_bytes().to_vec())),
                    ("tls.crt".to_string(), ByteString(self.cert.as_bytes().to_vec())),
                ])),
                metadata: ObjectMeta {
                    annotations: None,
                    labels: None,
                    name: Some(tls_name),
                    ..Default::default()
                },
                type_: Some("kubernetes.io/tls".to_string()),
                ..Default::default()
            },
        )
    }
}

impl SgTlsMode {
    pub(crate) fn to_kube_tls_mode_type(self) -> TlsModeType {
        match self {
            SgTlsMode::Terminate => "Terminate".to_string(),
            SgTlsMode::Passthrough => "Passthrough".to_string(),
        }
    }
}
