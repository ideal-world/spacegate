use crate::constants;
use crate::model::vo::Vo;
use kernel_common::inner_model::http_route::SgHttpRouteMatch;
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

use super::{backend_vo::SgBackendRefVo, plugin_vo::SgFilterVo};

/// HTTPRoute provides a way to route HTTP requests.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io%2fv1beta1.HTTPRoute)
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgHttpRouteVo {
    /// Name of the HttpRoute. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Associated gateway name.
    pub gateway_name: String,
    /// Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
    pub hostnames: Option<Vec<String>>,
    /// [crate::model::vo::plugin_vo::SgFilterVo]'s id
    pub filters: Vec<String>,
    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Vec<SgHttpRouteRuleVo>,

    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    #[serde(skip)]
    pub filter_vos: Vec<SgFilterVo>,
    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    #[serde(skip)]
    pub backend_vos: Vec<SgBackendRefVo>,
}

impl Vo for SgHttpRouteVo {
    fn get_vo_type() -> String {
        constants::ROUTE_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.name.clone()
    }
}

/// HTTPRouteRule defines semantics for matching an HTTP request based on conditions (matches), processing it (filters), and forwarding the request to an API object
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgHttpRouteRuleVo {
    /// Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
    pub matches: Option<Vec<SgHttpRouteMatch>>,
    /// [crate::model::vo::plugin_vo::SgFilterVo]'s id
    pub filters: Vec<String>,
    /// [crate::model::vo::backend_vo::SgBackendRefVo]'s id
    pub backends: Vec<String>,
    /// Timeout define the timeout for requests that match this rule.
    pub timeout_ms: Option<u64>,

    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    #[serde(skip)]
    pub filter_vos: Vec<SgFilterVo>,

    /// Parameters are only returned in the fn from_model() wrapper
    #[oai(skip)]
    #[serde(skip)]
    pub backend_vos: Vec<SgBackendRefVo>,
}
