use crate::model::vo::plugin_vo::SgFilterVo;
use crate::{constants, model::vo::Vo};
use kernel_common::inner_model::gateway::{SgParameters, SgProtocol, SgTls, SgTlsMode};
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

/// Gateway represents an instance of a service-traffic handling infrastructure
/// by binding Listeners to a set of IP addresses.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io/v1beta1.Gateway)
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgGatewayVo {
    /// Name of the Gateway. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Some parameters necessary for the gateway.
    pub parameters: SgParameters,
    /// Listeners associated with this Gateway. Listeners define logical endpoints
    /// that are bound on this Gateway’s addresses.
    pub listeners: Vec<SgListenerVo>,
    /// [crate::model::vo::plugin_vo::SgFilterVo]'s id
    pub filters: Vec<String>,
    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    pub tls: Vec<SgTls>,
    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    pub filter_vos: Vec<SgFilterVo>,
}

impl Vo for SgGatewayVo {
    fn get_vo_type() -> String {
        constants::GATEWAY_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.name.clone()
    }
}

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgListenerVo {
    /// Name is the name of the Listener. This name MUST be unique within a Gateway.
    pub name: String,
    /// Ip bound to the Listener. Default is 0.0.0.0
    pub ip: Option<String>,
    /// Port is the network port. Multiple listeners may use the same port, subject
    /// to the Listener compatibility rules.
    pub port: u16,
    /// Protocol specifies the network protocol this listener expects to receive.
    pub protocol: SgProtocol,
    pub tls: Option<SgTlsConfigVo>,
    /// `HostName` is used to define the host on which the listener accepts requests.
    pub hostname: Option<String>,
    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    pub tls_vo: Option<SgTls>,
}

#[derive(Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgTlsConfigVo {
    /// SgTlsConfigVo's name refers to the SecretVo.
    pub name: String,
    pub mode: SgTlsMode,
}
