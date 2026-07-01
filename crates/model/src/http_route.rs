use std::fmt::Display;

use crate::constants::DEFAULT_NAMESPACE;

pub use super::route_match::*;
use serde::{Deserialize, Serialize};

use super::{gateway::SgBackendProtocol, PluginInstanceId};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(untagged)]
pub enum SgRoute<P = PluginInstanceId> {
    Mcp(SgMcpRoute<P>),
    Http(SgHttpRoute<P>),
}

impl<P> SgRoute<P> {
    pub fn route_name(&self) -> &str {
        match self {
            SgRoute::Mcp(route) => &route.route_name,
            SgRoute::Http(route) => &route.route_name,
        }
    }

    pub fn map_plugins<F, T>(self, mut f: F) -> SgRoute<T>
    where
        F: FnMut(P) -> T,
    {
        match self {
            SgRoute::Mcp(route) => SgRoute::Mcp(route.map_plugins(&mut f)),
            SgRoute::Http(route) => SgRoute::Http(route.map_plugins(&mut f)),
        }
    }
}

impl<P> From<SgHttpRoute<P>> for SgRoute<P> {
    fn from(route: SgHttpRoute<P>) -> Self {
        SgRoute::Http(route)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub enum SgRouteKind {
    #[serde(rename = "MCPRoute")]
    McpRoute,
}

fn default_mcp_route_kind() -> SgRouteKind {
    SgRouteKind::McpRoute
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "ext-k8s", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SgMcpTransport {
    StreamableHttp,
    LegacySse,
}

impl Default for SgMcpTransport {
    fn default() -> Self {
        Self::StreamableHttp
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "ext-k8s", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimeoutMode {
    Request,
    Disabled,
}

impl Default for TimeoutMode {
    fn default() -> Self {
        Self::Request
    }
}

pub type McpTimeoutMode = TimeoutMode;

fn default_mcp_timeout_mode() -> TimeoutMode {
    TimeoutMode::Disabled
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "ext-k8s", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum McpSessionAffinity {
    McpSession,
    None,
}

impl Default for McpSessionAffinity {
    fn default() -> Self {
        Self::McpSession
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SgBalancePolicy {
    Random,
    IpHash,
    McpSession,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "ext-k8s", derive(schemars::JsonSchema))]
pub struct SgMcpLegacySse {
    pub sse_path: String,
    pub message_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct SgMcpRoute<P = PluginInstanceId> {
    #[serde(default = "default_mcp_route_kind")]
    pub kind: SgRouteKind,
    pub route_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostnames: Option<Vec<String>>,
    #[serde(default)]
    pub transport: SgMcpTransport,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_sse: Option<SgMcpLegacySse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backends: Vec<SgBackendRef<P>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<P>,
    #[serde(default = "default_mcp_timeout_mode")]
    pub timeout_mode: McpTimeoutMode,
    #[serde(default)]
    pub session_affinity: McpSessionAffinity,
}

impl<P> SgMcpRoute<P> {
    pub fn map_plugins<F, T>(self, mut f: F) -> SgMcpRoute<T>
    where
        F: FnMut(P) -> T,
    {
        SgMcpRoute {
            kind: self.kind,
            route_name: self.route_name,
            hostnames: self.hostnames,
            transport: self.transport,
            path: self.path,
            legacy_sse: self.legacy_sse,
            backends: self.backends.into_iter().map(|backend| backend.map_plugins(&mut f)).collect(),
            plugins: self.plugins.into_iter().map(&mut f).collect(),
            timeout_mode: self.timeout_mode,
            session_affinity: self.session_affinity,
        }
    }
}

/// HTTPRoute provides a way to route HTTP requests.
///
/// Reference: [Kubernetes Gateway](https://gateway-api.sigs.k8s.io/references/spec/#gateway.networking.k8s.io%2fv1beta1.HTTPRoute)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct SgHttpRoute<P = PluginInstanceId> {
    /// Route name
    pub route_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
    pub hostnames: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Filters define the filters that are applied to requests that match this hostnames.
    pub plugins: Vec<P>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Rules are a list of HTTP matchers, filters and actions.
    pub rules: Vec<SgHttpRouteRule<P>>,
    /// Rule priority, the rule of higher priority will be chosen.
    pub priority: i16,
}

impl<P> SgHttpRoute<P> {
    pub fn map_plugins<F, T>(self, mut f: F) -> SgHttpRoute<T>
    where
        F: FnMut(P) -> T,
    {
        SgHttpRoute {
            route_name: self.route_name,
            hostnames: self.hostnames,
            plugins: self.plugins.into_iter().map(&mut f).collect(),
            rules: self.rules.into_iter().map(|rule| rule.map_plugins(&mut f)).collect(),
            priority: self.priority,
        }
    }
}

impl<P> Default for SgHttpRoute<P> {
    fn default() -> Self {
        Self {
            route_name: Default::default(),
            hostnames: Default::default(),
            plugins: Default::default(),
            rules: Default::default(),
            priority: 1,
        }
    }
}

/// HTTPRouteRule defines semantics for matching an HTTP request based on conditions (matches), processing it (filters), and forwarding the request to an API object
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct SgHttpRouteRule<P = PluginInstanceId> {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
    pub matches: Option<Vec<SgHttpRouteMatch>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// Filters define the filters that are applied to requests that match this rule.
    pub plugins: Vec<P>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// BackendRefs defines the backend(s) where matching requests should be sent.
    pub backends: Vec<SgBackendRef<P>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Timeout define the timeout for requests that match this rule.
    pub timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_mode: Option<TimeoutMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_policy: Option<SgBalancePolicy>,
}

impl<P> SgHttpRouteRule<P> {
    pub fn map_plugins<F, T>(self, mut f: F) -> SgHttpRouteRule<T>
    where
        F: FnMut(P) -> T,
    {
        SgHttpRouteRule {
            matches: self.matches,
            plugins: self.plugins.into_iter().map(&mut f).collect(),
            backends: self.backends.into_iter().map(|backend| backend.map_plugins(&mut f)).collect(),
            timeout_ms: self.timeout_ms,
            timeout_mode: self.timeout_mode,
            balance_policy: self.balance_policy,
        }
    }
}

impl<P> Default for SgHttpRouteRule<P> {
    fn default() -> Self {
        Self {
            matches: Default::default(),
            plugins: Default::default(),
            backends: Default::default(),
            timeout_ms: Default::default(),
            timeout_mode: Default::default(),
            balance_policy: Default::default(),
        }
    }
}

/// BackendRef defines how a HTTPRoute should forward an HTTP request.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct SgBackendRef<P = PluginInstanceId> {
    // #[serde(flatten)]
    pub host: BackendHost,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Port specifies the destination port number to use for this resource.
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Timeout specifies the timeout for requests forwarded to the referenced backend.
    pub timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_mode: Option<TimeoutMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // Protocol specifies the protocol used to talk to the referenced backend.
    pub protocol: Option<SgBackendProtocol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Downgrade HTTP2 connections, it is useful when the backend does not support HTTP2.
    pub downgrade_http2: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Weight specifies the proportion of requests forwarded to the referenced backend.
    /// This is computed as weight/(sum of all weights in this BackendRefs list).
    /// For non-zero values, there may be some epsilon from the exact proportion defined here depending on the precision an implementation supports.
    /// Weight is not a percentage and the sum of weights does not need to equal 100.
    pub weight: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    /// plugins define the filters that are applied to backend that match this hostnames.
    ///
    /// # Notice!
    /// this field is ordered, the first plugin will be the outermost plugin.
    pub plugins: Vec<P>,
}

impl<P> SgBackendRef<P> {
    pub fn map_plugins<F, T>(self, f: F) -> SgBackendRef<T>
    where
        F: FnMut(P) -> T,
    {
        SgBackendRef {
            host: self.host,
            port: self.port,
            timeout_ms: self.timeout_ms,
            timeout_mode: self.timeout_mode,
            protocol: self.protocol,
            downgrade_http2: self.downgrade_http2,
            weight: self.weight,
            plugins: self.plugins.into_iter().map(f).collect(),
        }
    }

    pub fn get_host(&self) -> String {
        self.host.to_string()
    }
}

impl<P> Default for SgBackendRef<P> {
    fn default() -> Self {
        Self {
            host: Default::default(),
            port: Default::default(),
            timeout_ms: Default::default(),
            timeout_mode: Default::default(),
            downgrade_http2: Default::default(),
            protocol: Default::default(),
            weight: Default::default(),
            plugins: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
pub struct K8sServiceData {
    pub name: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
}

impl Display for K8sServiceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.namespace {
            Some(ref ns) => write!(f, "{}.{}", self.name, ns),
            None => write!(f, "{}.{}", self.name, DEFAULT_NAMESPACE),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind")]
pub enum BackendHost {
    Host { host: String },
    K8sService(K8sServiceData),
    File { path: String },
}

impl Display for BackendHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host { host } => write!(f, "{}", host),
            Self::K8sService(k8s_service) => write!(f, "{}", k8s_service),
            Self::File { path } => write!(f, "{}", path),
        }
    }
}

impl Default for BackendHost {
    fn default() -> Self {
        Self::Host { host: String::default() }
    }
}
