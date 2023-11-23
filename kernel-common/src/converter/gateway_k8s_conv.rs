use crate::client::k8s_client;
use crate::constants::k8s_constants::DEFAULT_NAMESPACE;
use crate::constants::k8s_constants::GATEWAY_CLASS_NAME;
use crate::converter::plugin_k8s_conv::SgSingeFilter;
use crate::helper::k8s_helper::{get_k8s_obj_unique, parse_k8s_obj_unique};
use crate::inner_model::gateway::{SgGateway, SgListener, SgParameters, SgProtocol, SgTls, SgTlsConfig, SgTlsMode};
use crate::k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef};
use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference, TlsModeType};
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::ByteString;
use kube::Api;
use std::collections::BTreeMap;
use std::str::FromStr;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

impl SgGateway {
    pub fn to_kube_gateway(self) -> (Gateway, Vec<SgSingeFilter>) {
        let (namespace, raw_name) = parse_k8s_obj_unique(&self.name);

        let gateway = Gateway {
            metadata: ObjectMeta {
                annotations: Some(self.parameters.to_kube_gateway()),
                labels: None,
                name: Some(raw_name),
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
                        tls: l.tls.map(|tls| {
                            let (namespace, name) = parse_k8s_obj_unique(&tls.tls.name);
                            GatewayTlsConfig {
                                mode: Some(tls.mode.to_kube_tls_mode_type()),
                                certificate_refs: Some(vec![SecretObjectReference {
                                    kind: Some("Secret".to_string()),
                                    name,
                                    namespace: Some(namespace),
                                    ..Default::default()
                                }]),
                                options: None,
                            }
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

        (gateway, sgfilters)
    }

    pub async fn from_kube_gateway(client_name: &str, gateway: Gateway) -> TardisResult<SgGateway> {
        //todo filters
        let filters = None;
        let result = SgGateway {
            name: get_k8s_obj_unique(&gateway),
            parameters: SgParameters::from_kube_gateway(&gateway),
            listeners: join_all(
                gateway
                    .spec
                    .listeners
                    .into_iter()
                    .map(|listener| async move {
                        let tls = match listener.tls {
                            Some(tls_config) => {
                                if let Some(tls) = SgTls::from_kube_tls(client_name, tls_config.certificate_refs).await? {
                                    Some(SgTlsConfig {
                                        mode: SgTlsMode::from(tls_config.mode).unwrap_or_default(),
                                        tls,
                                    })
                                } else {
                                    None
                                }
                            }
                            None => None,
                        };
                        let sg_listener = SgListener {
                            name: listener.name,
                            ip: None,
                            port: listener.port,
                            protocol: match listener.protocol.to_lowercase().as_str() {
                                "http" => SgProtocol::Http,
                                "https" => SgProtocol::Https,
                                "ws" => SgProtocol::Ws,
                                _ => {
                                    return Err(TardisError::not_implemented(
                                        &format!("[SG.Config] Gateway [spec.listener.protocol={}] not supported yet", listener.protocol),
                                        "",
                                    ))
                                }
                            },
                            tls,
                            hostname: listener.hostname,
                        };
                        Ok(sg_listener)
                    })
                    .collect::<Vec<_>>(),
            )
            .await
            .into_iter()
            .map(|listener| listener.expect("[SG.Config] Unexpected none: listener"))
            .collect(),
            filters,
        };
        Ok(result)
    }
}

impl SgParameters {
    pub(crate) fn to_kube_gateway(self) -> BTreeMap<String, String> {
        let mut ann = BTreeMap::new();
        if let Some(redis_url) = self.redis_url {
            ann.insert(crate::constants::k8s_constants::GATEWAY_ANNOTATION_REDIS_URL.to_string(), redis_url);
        }
        if let Some(log_level) = self.log_level {
            ann.insert(crate::constants::k8s_constants::GATEWAY_ANNOTATION_LOG_LEVEL.to_string(), log_level);
        }
        if let Some(lang) = self.lang {
            ann.insert(crate::constants::k8s_constants::GATEWAY_ANNOTATION_LANGUAGE.to_string(), lang);
        }
        if let Some(ignore_tls_verification) = self.ignore_tls_verification {
            ann.insert(
                crate::constants::k8s_constants::GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION.to_string(),
                ignore_tls_verification.to_string(),
            );
        }
        ann
    }

    pub fn from_kube_gateway(gateway: &Gateway) -> Self {
        let gateway_annotations = gateway.metadata.annotations.clone();
        if let Some(gateway_annotations) = gateway_annotations {
            SgParameters {
                redis_url: gateway_annotations.get(crate::constants::k8s_constants::GATEWAY_ANNOTATION_REDIS_URL).map(|v| v.to_string()),
                log_level: gateway_annotations.get(crate::constants::k8s_constants::GATEWAY_ANNOTATION_LOG_LEVEL).map(|v| v.to_string()),
                lang: gateway_annotations.get(crate::constants::k8s_constants::GATEWAY_ANNOTATION_LANGUAGE).map(|v| v.to_string()),
                ignore_tls_verification: gateway_annotations.get(crate::constants::k8s_constants::GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION).and_then(|v| v.parse::<bool>().ok()),
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

impl SgTls {
    pub fn to_kube_tls(self) -> Secret {
        let (namespace, raw_name) = parse_k8s_obj_unique(&self.name);
        Secret {
            data: Some(BTreeMap::from([
                ("tls.key".to_string(), ByteString(self.key.as_bytes().to_vec())),
                ("tls.crt".to_string(), ByteString(self.cert.as_bytes().to_vec())),
            ])),
            metadata: ObjectMeta {
                annotations: None,
                labels: None,
                name: Some(raw_name),
                namespace: Some(namespace),
                ..Default::default()
            },
            type_: Some("kubernetes.io/tls".to_string()),
            ..Default::default()
        }
    }

    pub async fn from_kube_tls(client_name: &str, tls: Option<Vec<SecretObjectReference>>) -> TardisResult<Option<Self>> {
        let certificate_ref = tls
            .as_ref()
            .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is required", ""))?
            .get(0)
            .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is empty", ""))?;

        let secret_api: Api<Secret> = Api::namespaced(
            (*k8s_client::get(Some(&client_name.to_string())).await?).clone(),
            certificate_ref.namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()),
        );
        let result = if let Some(secret_obj) =
            secret_api.get_opt(&certificate_ref.name).await.map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        {
            let secret_un = get_k8s_obj_unique(&secret_obj);
            let secret_data =
                secret_obj.data.ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data is required", certificate_ref.name), ""))?;
            let tls_crt = secret_data
                .get("tls.crt")
                .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.crt] is required", certificate_ref.name), ""))?;
            let tls_key = secret_data
                .get("tls.key")
                .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.key] is required", certificate_ref.name), ""))?;
            Some(SgTls {
                name: secret_un,
                key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
            })
        } else {
            TardisError::not_found(&format!("[SG.admin] Gateway have tls secret [{}], but not found!", certificate_ref.name), "");
            None
        };
        Ok(result)
    }
}

impl SgTlsMode {
    pub fn to_kube_tls_mode_type(self) -> TlsModeType {
        match self {
            SgTlsMode::Terminate => "Terminate".to_string(),
            SgTlsMode::Passthrough => "Passthrough".to_string(),
        }
    }

    pub fn from(mode: Option<String>) -> Option<Self> {
        if let Some(mode) = mode {
            if let Ok(mode) = SgTlsMode::from_str(&mode) {
                return Some(mode);
            }
        }
        None
    }
}
