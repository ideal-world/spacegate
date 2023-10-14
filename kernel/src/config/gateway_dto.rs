use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use tardis::basic::error::TardisError;

use super::plugin_filter_dto::SgRouteFilter;

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgGateway {
    /// Name of the Gateway. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Some parameters necessary for the gateway.
    pub parameters: SgParameters,
    /// Listeners associated with this Gateway. Listeners define logical endpoints that are bound on this Gateway’s addresses.
    pub listeners: Vec<SgListener>,
    /// Filters define the filters that are applied to requests that match this gateway.
    pub filters: Option<Vec<SgRouteFilter>>,
}

/// Gateway parameter configuration.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
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

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgListener {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: Option<String>,
    /// Ip bound to the Listener. Default is 0.0.0.0
    pub ip: Option<String>,
    /// Port is the network port. Multiple listeners may use the same port, subject to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    /// TLS is the TLS configuration for the Listener.
    /// This field is required if the Protocol field is “HTTPS” or “TLS”. It is invalid to set this field if the Protocol field is “HTTP”, “TCP”, or “UDP”.
    pub tls: Option<SgTlsConfig>,
    /// `HostName` is used to define the host on which the listener accepts requests.
    pub hostname: Option<String>,
}

/// ProtocolType defines the application protocol accepted by a Listener.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SgProtocol {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support HTTP/2 over cleartext.
    /// If implementations support HTTP/2 over cleartext on “HTTP” listeners, that MUST be clearly documented by the implementation.
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
pub struct SgTlsConfig {
    pub mode: SgTlsMode,
    pub key: String,
    pub cert: String,
}

#[derive(Debug, Serialize, PartialEq, Deserialize, Clone, Default)]
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
}
