#[cfg(feature = "k8s")]
use k8s_gateway_api::{Gateway, GatewaySpec, GatewayTlsConfig, Listener, SecretObjectReference, TlsModeType};
#[cfg(feature = "k8s")]
use k8s_openapi::api::core::v1::Secret;
#[cfg(feature = "k8s")]
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
#[cfg(feature = "k8s")]
use k8s_openapi::ByteString;
#[cfg(feature = "k8s")]
use std::collections::BTreeMap;
use std::{fmt::Display, str::FromStr};

use super::plugin_filter_dto::SgRouteFilter;
#[cfg(feature = "k8s")]
use crate::constants::GATEWAY_CLASS_NAME;
#[cfg(feature = "k8s")]
use crate::dto::plugin_filter_dto::SgSingeFilter;
#[cfg(feature = "k8s")]
use crate::k8s_crd::sg_filter::{K8sSgFilterSpecFilter, K8sSgFilterSpecTargetRef};
use serde::{Deserialize, Serialize};
use tardis::basic::error::TardisError;
#[cfg(feature = "admin-support")]
use tardis::web::poem_openapi;
#[cfg(feature = "k8s")]
use tardis::TardisFuns;

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Object))]
pub struct SgGateway {
    /// Name of the Gateway. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Some parameters necessary for the gateway.
    pub parameters: SgParameters,
    /// Listeners associated with this Gateway. Listeners define logical endpoints
    /// that are bound on this Gateway’s addresses.
    pub listeners: Vec<SgListener>,
    /// Filters define the filters that are applied to requests that match this gateway.
    pub filters: Option<Vec<SgRouteFilter>>,
}

impl SgGateway {
    #[cfg(feature = "k8s")]
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

/// Gateway parameter configuration.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Object))]
pub struct SgParameters {
    /// Redis access Url, Url with permission information.
    pub redis_url: Option<String>,
    /// Gateway Log_Level
    pub log_level: Option<String>,
    /// Gateway language
    pub lang: Option<String>,
    /// Ignore backend tls verification
    pub ignore_tls_verification: Option<bool>,
}

impl SgParameters {
    #[cfg(feature = "k8s")]
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

    #[cfg(feature = "k8s")]
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

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Object))]
pub struct SgListener {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    /// Ip bound to the Listener. Default is 0.0.0.0
    pub ip: Option<String>,
    /// Port is the network port. Multiple listeners may use the same port, subject
    /// to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    /// TLS is the TLS configuration for the Listener.
    /// This field is required if the Protocol field is “HTTPS” or “TLS”. It is invalid
    /// to set this field if the Protocol field is “HTTP”, “TCP”, or “UDP”.
    pub tls: Option<SgTlsConfig>,
    /// `HostName` is used to define the host on which the listener accepts requests.
    pub hostname: Option<String>,
}

/// ProtocolType defines the application protocol accepted by a Listener.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Enum))]
#[serde(rename_all = "lowercase")]
pub enum SgProtocol {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support
    /// HTTP/2 over cleartext.
    /// If implementations support HTTP/2 over cleartext on “HTTP” listeners, that
    /// MUST be clearly documented by the implementation.
    #[default]
    Http,
    /// Accepts HTTP/1.1 or HTTP/2 sessions over TLS.
    Https,
    Ws,
    Wss,
}

impl Display for SgProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SgProtocol::Http => write!(f, "http"),
            SgProtocol::Https => write!(f, "https"),
            SgProtocol::Ws => write!(f, "ws"),
            SgProtocol::Wss => write!(f, "wss"),
        }
    }
}

/// GatewayTLSConfig describes a TLS configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Object))]
pub struct SgTlsConfig {
    pub mode: SgTlsMode,
    pub key: String,
    pub cert: String,
}

impl SgTlsConfig {
    #[cfg(feature = "k8s")]
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

#[derive(Debug, Serialize, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Enum))]
pub enum SgTlsMode {
    Terminate,
    #[default]
    Passthrough,
}

impl FromStr for SgTlsMode {
    type Err = TardisError;
    fn from_str(mode: &str) -> Result<SgTlsMode, Self::Err> {
        let level = mode.to_lowercase();
        match level.as_str() {
            "terminate" => Ok(SgTlsMode::Terminate),
            "passthrough" => Ok(SgTlsMode::Passthrough),
            _ => Err(TardisError::bad_request("SgTlsMode parse error", "")),
        }
    }
}

impl SgTlsMode {
    pub fn from(mode: Option<String>) -> Option<Self> {
        if let Some(mode) = mode {
            if let Ok(mode) = SgTlsMode::from_str(&mode) {
                return Some(mode);
            }
        }
        None
    }

    #[cfg(feature = "k8s")]
    pub(crate) fn to_kube_tls_mode_type(self) -> TlsModeType {
        match self {
            SgTlsMode::Terminate => "Terminate".to_string(),
            SgTlsMode::Passthrough => "Passthrough".to_string(),
        }
    }
}
