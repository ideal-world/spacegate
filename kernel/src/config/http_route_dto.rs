use serde::{Deserialize, Serialize};
pub use spacegate_tower::layers::http_route::match_request::*;

use super::{gateway_dto::SgProtocol, plugin_filter_dto::SgRouteFilter};

/// HTTPRoute provides a way to route HTTP requests.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io%2fv1beta1.HTTPRoute)
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpRoute {
    /// Associated gateway name.
    pub gateway_name: String,
    /// Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
    pub hostnames: Option<Vec<String>>,
    /// Filters define the filters that are applied to requests that match this hostnames.
    pub filters: Option<Vec<SgRouteFilter>>,
    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Option<Vec<SgHttpRouteRule>>,
}

/// HTTPRouteRule defines semantics for matching an HTTP request based on conditions (matches), processing it (filters), and forwarding the request to an API object
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpRouteRule {
    /// Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
    pub matches: Option<Vec<SgHttpRouteMatch>>,
    /// Filters define the filters that are applied to requests that match this rule.
    pub filters: Option<Vec<SgRouteFilter>>,
    /// BackendRefs defines the backend(s) where matching requests should be sent.
    pub backends: Option<Vec<SgBackendRef>>,
    /// Timeout define the timeout for requests that match this rule.
    pub timeout_ms: Option<u64>,
}

/// BackendRef defines how a HTTPRoute should forward an HTTP request.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgBackendRef {
    /// Name is the kubernetes service name OR url host.
    pub name_or_host: String,
    /// Namespace is the kubernetes namespace
    pub namespace: Option<String>,
    /// Port specifies the destination port number to use for this resource.
    pub port: u16,
    /// Timeout specifies the timeout for requests forwarded to the referenced backend.
    pub timeout_ms: Option<u64>,
    // Protocol specifies the protocol used to talk to the referenced backend.
    pub protocol: Option<SgProtocol>,
    /// Weight specifies the proportion of requests forwarded to the referenced backend.
    /// This is computed as weight/(sum of all weights in this BackendRefs list).
    /// For non-zero values, there may be some epsilon from the exact proportion defined here depending on the precision an implementation supports.
    /// Weight is not a percentage and the sum of weights does not need to equal 100.
    pub weight: Option<u16>,
    /// Filters define the filters that are applied to backend that match this hostnames.
    pub filters: Option<Vec<SgRouteFilter>>,
}

impl SgBackendRef {
    pub fn get_host(&self) -> String {
        match self.namespace {
            Some(ref ns) => format!("{}.{}", self.name_or_host, ns),
            None => self.name_or_host.clone(),
        }
    }
}
