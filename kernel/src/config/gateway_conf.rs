use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgGateway {}

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgListener {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    /// Port is the network port. Multiple listeners may use the same port, subject to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    pub tls:SgTlsConfig,
}

/// ProtocolType defines the application protocol accepted by a Listener.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SgProtocol {
    /// Accepts cleartext HTTP/1.1 sessions over TCP. Implementations MAY also support HTTP/2 over cleartext. If implementations support HTTP/2 over cleartext on “HTTP” listeners, that MUST be clearly documented by the implementation.
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
    pub nameOrPath:Option<String>,
    pub key: Option<String>,
    pub cert:Option<String>
}



