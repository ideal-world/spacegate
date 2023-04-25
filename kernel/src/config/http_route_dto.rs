use serde::{Deserialize, Serialize};

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
    pub backends: Option<Vec<SgHttpBackendRef>>,
}

/// HTTPRouteMatch defines the predicate used to match requests to a given action. Multiple match types are ANDed together, i.e. the match will evaluate to true only if all conditions are satisfied.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpRouteMatch {
    /// Path specifies a HTTP request path matcher. If this field is not specified, a default prefix match on the “/” path is provided.
    pub path: Option<SgHttpPathMatch>,
    /// Headers specifies HTTP request header matchers. Multiple match values are ANDed together, meaning, a request must match all the specified headers to select the route.
    pub header: Option<Vec<SgHttpHeaderMatch>>,
    /// Query specifies HTTP query parameter matchers. Multiple match values are ANDed together, meaning, a request must match all the specified query parameters to select the route.
    pub query: Option<Vec<SgHttpQueryMatch>>,
    /// Method specifies HTTP method matcher. When specified, this route will be matched only if the request has the specified method.
    pub method: Option<Vec<String>>,
}

/// HTTPPathMatch describes how to select a HTTP route by matching the HTTP request path.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpPathMatch {
    /// Type specifies how to match against the path Value.
    pub kind: SgHttpPathMatchType,
    /// Value of the HTTP path to match against.
    pub value: String,
}

/// PathMatchType specifies the semantics of how HTTP paths should be compared.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgHttpPathMatchType {
    /// Matches the URL path exactly and with case sensitivity.
    Exact,
    /// Matches based on a URL path prefix split by /. Matching is case sensitive and done on a path element by element basis.
    /// A path element refers to the list of labels in the path split by the / separator. When specified, a trailing / is ignored.
    #[default]
    Prefix,
    /// Matches if the URL path matches the given regular expression with case sensitivity.
    Regular,
}

/// HTTPHeaderMatch describes how to select a HTTP route by matching HTTP request headers.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpHeaderMatch {
    /// Type specifies how to match against the value of the header.
    pub kind: SgHttpHeaderMatchType,
    /// Name is the name of the HTTP Header to be matched. Name matching MUST be case insensitive. (See https://tools.ietf.org/html/rfc7230#section-3.2).
    pub name: String,
    /// Value is the value of HTTP Header to be matched.
    pub value: String,
}

/// HeaderMatchType specifies the semantics of how HTTP header values should be compared.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgHttpHeaderMatchType {
    /// Matches the HTTP header exactly and with case sensitivity.
    #[default]
    Exact,
    /// Matches if the Http header matches the given regular expression with case sensitivity.
    Regular,
}

/// HTTPQueryMatch describes how to select a HTTP route by matching HTTP query parameters.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpQueryMatch {
    /// Type specifies how to match against the value of the query parameter.
    pub kind: SgHttpQueryMatchType,
    /// Name is the name of the HTTP query param to be matched. This must be an exact string match. (See https://tools.ietf.org/html/rfc7230#section-2.7.3).
    pub name: String,
    /// Value is the value of HTTP query param to be matched.
    pub value: String,
}

/// HTTPQueryMatchType specifies the semantics of how HTTP query parameter values should be compared.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgHttpQueryMatchType {
    /// Matches the HTTP query parameter exactly and with case sensitivity.
    #[default]
    Exact,
    /// Matches if the Http query parameter matches the given regular expression with case sensitivity.
    Regular,
}

/// HTTPBackendRef defines how a HTTPRoute should forward an HTTP request.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpBackendRef {
    /// Name is the kubernetes service name OR url path.
    pub name_or_path: String,
    /// Namespace is the kubernetes namespace Or url host
    pub namespace_or_host: Option<String>,
    /// Port specifies the destination port number to use for this resource.
    pub port: u16,
    // Protocol specifies the protocol used to talk to the referenced backend.
    pub protocol: Option<SgProtocol>,
    /// Weight specifies the proportion of requests forwarded to the referenced backend.
    /// This is computed as weight/(sum of all weights in this BackendRefs list).
    /// For non-zero values, there may be some epsilon from the exact proportion defined here depending on the precision an implementation supports.
    /// Weight is not a percentage and the sum of weights does not need to equal 100.
    pub weight: Option<i32>,
}
