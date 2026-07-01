use k8s_gateway_api::{CommonRouteSpec, Hostname, RouteStatus};

use crate::{constants::DEFAULT_NAMESPACE, McpSessionAffinity, McpTimeoutMode, SgMcpLegacySse, SgMcpTransport, TimeoutMode};

use super::http_spaceroute::HttpBackendRef;

#[derive(Clone, Debug, kube::CustomResource, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[kube(
    group = "spacegate.idealworld.group",
    version = "v1",
    kind = "MCPRoute",
    struct = "McpRoute",
    status = "McpRouteStatus",
    namespaced
)]
pub struct McpRouteSpec {
    #[serde(flatten)]
    pub inner: CommonRouteSpec,
    pub hostnames: Option<Vec<Hostname>>,
    #[serde(default)]
    pub transport: SgMcpTransport,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_sse: Option<SgMcpLegacySse>,
    #[serde(default)]
    pub backend_refs: Vec<HttpBackendRef>,
    #[serde(default = "default_mcp_timeout_mode")]
    pub timeout_mode: McpTimeoutMode,
    #[serde(default)]
    pub session_affinity: McpSessionAffinity,
}

fn default_mcp_timeout_mode() -> TimeoutMode {
    TimeoutMode::Disabled
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct McpRouteStatus {
    #[serde(flatten)]
    pub inner: RouteStatus,
}

impl McpRoute {
    pub fn get_gateway_name(&self, namespace: &str) -> String {
        self.spec
            .inner
            .parent_refs
            .as_ref()
            .map(|parent_refs| {
                parent_refs
                    .iter()
                    .filter(|parent_ref| parent_ref.namespace.eq(&Some(namespace.to_string())) || (namespace == DEFAULT_NAMESPACE && parent_ref.namespace.is_none()))
                    .map(|parent_ref| parent_ref.name.clone())
                    .next()
            })
            .unwrap_or_default()
            .unwrap_or_default()
    }
}
