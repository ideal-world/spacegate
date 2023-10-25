use crate::{constants, model::vo::Vo};
use kernel_common::inner_model::gateway::{SgParameters, SgProtocol, SgTlsMode};
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgGatewayVO {
    /// Name of the Gateway. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Some parameters necessary for the gateway.
    pub parameters: SgParameters,
    /// Listeners associated with this Gateway. Listeners define logical endpoints
    /// that are bound on this Gateway’s addresses.
    pub listeners: Vec<SgListenerVO>,
    /// [crate::model::vo::plugin_vo::SgFilterVO]'s id
    pub filters: Option<Vec<String>>,
}

impl Vo for SgGatewayVO {
    fn get_vo_type() -> String {
        constants::GATEWAY_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.name.clone()
    }
}

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgListenerVO {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    /// Ip bound to the Listener. Default is 0.0.0.0
    pub ip: Option<String>,
    /// Port is the network port. Multiple listeners may use the same port, subject
    /// to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    /// SgTlsConfig's id refers to the TLS configuration.
    pub tls: Option<String>,
    /// `HostName` is used to define the host on which the listener accepts requests.
    pub hostname: Option<String>,
}

/// GatewayTLSConfig describes a TLS configuration.
/// unique by id
#[derive(Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgTlsConfigVO {
    pub id: String,
    pub mode: SgTlsMode,
    pub key: String,
    pub cert: String,
    pub ref_ids: Option<Vec<String>>,
}

impl Vo for SgTlsConfigVO {
    fn get_vo_type() -> String {
        constants::TLS_CONFIG_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.id.clone()
    }
}
