use kernel_common::inner_model::http_route::SgHttpRouteMatch;
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

/// HTTPRoute provides a way to route HTTP requests.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io%2fv1beta1.HTTPRoute)
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgHttpRoute {
    /// Name of the HttpRoute. Global Unique.
    ///
    /// In k8s mode, this name MUST be unique within a namespace.
    /// format see [k8s_helper::format_k8s_obj_unique]
    pub name: String,
    /// Associated gateway name.
    pub gateway_name: String,
    /// Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
    pub hostnames: Option<Vec<String>>,
    /// [crate::model::vo::plugin_vo::SgFilterVO]'s id
    pub filters: Option<Vec<String>>,
    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Option<Vec<SgHttpRouteRuleVO>>,
}

/// HTTPRouteRule defines semantics for matching an HTTP request based on conditions (matches), processing it (filters), and forwarding the request to an API object
#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgHttpRouteRuleVO {
    /// Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
    pub matches: Option<Vec<SgHttpRouteMatch>>,
    /// [crate::model::vo::plugin_vo::SgFilterVO]'s id
    pub filters: Option<Vec<String>>,
    /// [crate::model::vo::backend_vo::BackendRefVO]'s id
    pub backends: Option<Vec<String>>,
    /// Timeout define the timeout for requests that match this rule.
    pub timeout_ms: Option<u64>,
}
