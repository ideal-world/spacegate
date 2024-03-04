pub use super::route_match::*;
use serde::{Deserialize, Serialize};

use super::{filter::SgRouteFilter, gateway::SgBackendProtocol};

/// HTTPRoute provides a way to route HTTP requests.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io%2fv1beta1.HTTPRoute)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(default)]
pub struct SgHttpRoute {
    /// Associated gateway name.
    pub gateway_name: String,
    /// Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
    pub hostnames: Option<Vec<String>>,
    /// Filters define the filters that are applied to requests that match this hostnames.
    pub filters: Vec<SgRouteFilter>,
    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Vec<SgHttpRouteRule>,
    /// Rule priority, the rule of higher priority will be chosen.
    pub priority: u16,
}

impl Default for SgHttpRoute {
    fn default() -> Self {
        Self {
            gateway_name: Default::default(),
            hostnames: Default::default(),
            filters: Default::default(),
            rules: Default::default(),
            priority: 1,
        }
    }
}

/// HTTPRouteRule defines semantics for matching an HTTP request based on conditions (matches), processing it (filters), and forwarding the request to an API object
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(default)]
pub struct SgHttpRouteRule {
    /// Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
    pub matches: Option<Vec<SgHttpRouteMatch>>,
    /// Filters define the filters that are applied to requests that match this rule.
    pub filters: Vec<SgRouteFilter>,
    /// BackendRefs defines the backend(s) where matching requests should be sent.
    pub backends: Vec<SgBackendRef>,
    /// Timeout define the timeout for requests that match this rule.
    pub timeout_ms: Option<u32>,
}

/// BackendRef defines how a HTTPRoute should forward an HTTP request.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(default)]
pub struct SgBackendRef {
    // #[serde(flatten)]
    pub host: BackendHost,
    /// Port specifies the destination port number to use for this resource.
    pub port: u16,
    /// Timeout specifies the timeout for requests forwarded to the referenced backend.
    pub timeout_ms: Option<u32>,
    // Protocol specifies the protocol used to talk to the referenced backend.
    pub protocol: Option<SgBackendProtocol>,
    /// Weight specifies the proportion of requests forwarded to the referenced backend.
    /// This is computed as weight/(sum of all weights in this BackendRefs list).
    /// For non-zero values, there may be some epsilon from the exact proportion defined here depending on the precision an implementation supports.
    /// Weight is not a percentage and the sum of weights does not need to equal 100.
    pub weight: u16,
    /// Filters define the filters that are applied to backend that match this hostnames.
    pub filters: Vec<SgRouteFilter>,
}

impl Default for SgBackendRef {
    fn default() -> Self {
        Self {
            host: Default::default(),
            port: 80,
            timeout_ms: Default::default(),
            protocol: Default::default(),
            weight: 1,
            filters: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
pub struct K8sServiceData {
    pub name: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
}

impl ToString for K8sServiceData {
    fn to_string(&self) -> String {
        match self.namespace {
            Some(ref ns) => format!("{}.{}", self.name, ns),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[serde(tag = "kind")]
pub enum BackendHost {
    Host {
        host: String,
    },
    #[cfg(feature = "k8s")]
    K8sService(K8sServiceData),
}

impl ToString for BackendHost {
    fn to_string(&self) -> String {
        match self {
            Self::Host { host } => host.clone(),
            #[cfg(feature = "k8s")]
            Self::K8sService(k8s_service) => k8s_service.to_string(),
        }
    }
}

impl Default for BackendHost {
    fn default() -> Self {
        Self::Host { host: String::default() }
    }
}

impl SgBackendRef {
    pub fn get_host(&self) -> String {
        self.host.to_string()
    }
}
