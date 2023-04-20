use serde::{Deserialize, Serialize};

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgGateway {
    /// Some parameters necessary for the gateway.
    parameters: SgParameters,
    /// Listeners associated with this Gateway. Listeners define logical endpoints that are bound on this Gateway’s addresses.
    listeners: Vec<SgListener>,
}

/// Gateway parameter configuration.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgParameters {
    /// Redis access Url, Url with permission information.
    pub redis_url: String,
}

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgListener {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    /// Port is the network port. Multiple listeners may use the same port, subject to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    /// TLS is the TLS configuration for the Listener.
    /// This field is required if the Protocol field is “HTTPS” or “TLS”. It is invalid to set this field if the Protocol field is “HTTP”, “TCP”, or “UDP”.
    pub tls: Option<SgTlsConfig>,
}

/// ProtocolType defines the application protocol accepted by a Listener.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum SgProtocol {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support HTTP/2 over cleartext.
    /// If implementations support HTTP/2 over cleartext on “HTTP” listeners, that MUST be clearly documented by the implementation.
    Http,
    /// Accepts HTTP/1.1 or HTTP/2 sessions over TLS.
    Https,
}

impl Default for SgProtocol {
    fn default() -> Self {
        SgProtocol::Http
    }
}

/// GatewayTLSConfig describes a TLS configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgTlsConfig {
    pub name: String,
    pub key: Option<String>,
    pub cert: Option<String>,
}
