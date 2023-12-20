use crate::constants;
use crate::model::vo::Vo;
use kernel_common::inner_model::gateway::SgProtocol;
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

use super::plugin_vo::SgFilterVo;

/// BackendRef defines how a HTTPRoute should forward an HTTP request.
#[derive(Default, Debug, Eq, Hash, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgBackendRefVo {
    /// unique by id
    pub id: String,
    /// Name is the kubernetes service name OR url host.
    pub name_or_host: String,
    /// Namespace is the kubernetes namespace
    pub namespace: Option<String>,
    /// Port specifies the destination port number to use for this resource.
    pub port: u16,
    /// Timeout specifies the timeout for requests forwarded to the referenced backend.
    pub timeout_ms: Option<u64>,
    /// Protocol specifies the protocol used to talk to the referenced backend.
    pub protocol: Option<SgProtocol>,
    /// Weight specifies the proportion of requests forwarded to the referenced backend.
    /// This is computed as weight/(sum of all weights in this BackendRefs list).
    /// For non-zero values, there may be some epsilon from the exact proportion defined here depending on the precision an implementation supports.
    /// Weight is not a percentage and the sum of weights does not need to equal 100.
    pub weight: Option<u16>,
    /// [crate::model::vo::plugin_vo::SgFilterVo]'s id
    pub filters: Option<Vec<String>>,

    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    #[serde(skip)]
    pub filter_vos: Vec<SgFilterVo>,
}

impl PartialEq for SgBackendRefVo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Vo for SgBackendRefVo {
    fn get_vo_type() -> String {
        constants::BACKEND_REF_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.id.clone()
    }
}
