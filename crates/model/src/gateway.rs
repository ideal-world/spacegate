use std::{fmt::Display, net::IpAddr};

use serde::{Deserialize, Serialize};

use super::plugin::PluginInstanceId;

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
/// Name of the Gateway. Global Unique.
pub struct SgGateway<P = PluginInstanceId> {
    pub name: String,
    /// Some parameters necessary for the gateway.
    pub parameters: SgParameters,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Listeners associated with this Gateway. Listeners define logical endpoints that are bound on this Gateway’s addresses.
    pub listeners: Vec<SgListener>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Filters define the filters that are applied to requests that match this gateway.
    pub plugins: Vec<P>,
}

impl<P> Default for SgGateway<P> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            parameters: Default::default(),
            listeners: Default::default(),
            plugins: Default::default(),
        }
    }
}

impl<P> SgGateway<P> {
    pub fn map_plugins<F, T>(self, f: F) -> SgGateway<T>
    where
        F: FnMut(P) -> T,
    {
        SgGateway {
            name: self.name,
            parameters: self.parameters,
            listeners: self.listeners,
            plugins: self.plugins.into_iter().map(f).collect(),
        }
    }
}

/// Gateway parameter configuration.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct SgParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Redis access Url, Url with permission information.
    pub redis_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Gateway Log_Level
    pub log_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Gateway language
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Ignore backend tls verification
    pub ignore_tls_verification: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Add request id for every request
    pub enable_x_request_id: Option<bool>,
}

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct SgListener {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Ip bound to the Listener. Default is 0.0.0.0
    pub ip: Option<IpAddr>,
    /// Port is the network port. Multiple listeners may use the same port, subject to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocolConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// `HostName` is used to define the host on which the listener accepts requests.
    pub hostname: Option<String>,
}

#[non_exhaustive]
/// ProtocolType defines the application protocol accepted by a Listener.
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SgBackendProtocol {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support HTTP/2 over cleartext.
    /// If implementations support HTTP/2 over cleartext on “HTTP” listeners, that MUST be clearly documented by the implementation.
    #[default]
    Http,
    /// Accepts HTTP/1.1 or HTTP/2 sessions over TLS.
    Https,
}

impl Display for SgBackendProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SgBackendProtocol::Http => write!(f, "http"),
            SgBackendProtocol::Https { .. } => write!(f, "https"),
        }
    }
}

#[non_exhaustive]
/// ProtocolType defines the application protocol accepted by a Listener.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "lowercase", tag = "type")]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgProtocolConfig {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support HTTP/2 over cleartext.
    /// If implementations support HTTP/2 over cleartext on “HTTP” listeners, that MUST be clearly documented by the implementation.
    #[default]
    Http,
    /// Accepts HTTP/1.1 or HTTP/2 sessions over TLS.
    Https {
        /// TLS is the TLS configuration for the Listener.
        /// This field is required if the Protocol field is “HTTPS” or “TLS”. It is invalid to set this field if the Protocol field is “HTTP”, “TCP”, or “UDP”.
        tls: SgTlsConfig,
    },
}

impl Display for SgProtocolConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SgProtocolConfig::Http => write!(f, "http"),
            SgProtocolConfig::Https { .. } => write!(f, "https"),
        }
    }
}

/// GatewayTLSConfig describes a TLS configuration.
#[derive(Debug, Serialize, PartialEq, Eq, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct SgTlsConfig {
    pub mode: SgTlsMode,
    pub key: String,
    pub cert: String,
}

#[derive(Debug, Serialize, PartialEq, Deserialize, Clone, Default, Eq, Copy)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgTlsMode {
    Terminate,
    #[default]
    Passthrough,
}

impl From<SgTlsMode> for String {
    fn from(value: SgTlsMode) -> Self {
        match value {
            SgTlsMode::Terminate => "Terminate".to_string(),
            SgTlsMode::Passthrough => "Passthrough".to_string(),
        }
    }
}

impl From<String> for SgTlsMode {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "terminate" => SgTlsMode::Terminate,
            "passthrough" => SgTlsMode::Passthrough,
            _ => SgTlsMode::Passthrough,
        }
    }
}

impl From<Option<String>> for SgTlsMode {
    fn from(value: Option<String>) -> Self {
        SgTlsMode::from(value.unwrap_or_default())
    }
}

impl SgTlsMode {
    pub fn to_pascal_case(&self) -> &'static str {
        match self {
            SgTlsMode::Terminate => "Terminate",
            SgTlsMode::Passthrough => "Passthrough",
        }
    }
}
