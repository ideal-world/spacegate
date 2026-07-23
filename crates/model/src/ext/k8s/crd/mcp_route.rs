use k8s_gateway_api::{CommonRouteSpec, Hostname, RouteStatus};

use crate::{constants::DEFAULT_NAMESPACE, McpSessionAffinity, McpTimeoutMode, SgMcpLegacySse, SgMcpTransport, TimeoutMode};

use super::http_spaceroute::{BackendRef, HttpBackendRef};

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
    #[schemars(schema_with = "mcp_backend_refs_schema")]
    pub backend_refs: Vec<HttpBackendRef>,
    #[serde(default = "default_mcp_timeout_mode")]
    pub timeout_mode: McpTimeoutMode,
    #[serde(default)]
    pub session_affinity: McpSessionAffinity,
}

#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct McpBackendRefSchema {
    /// Backend target fields used by MCPRoute.
    #[serde(flatten)]
    backend_ref: Option<BackendRef>,
    /// HTTPRoute-compatible backend filters are validated by the runtime model.
    filters: Option<Vec<McpBackendFilterSchema>>,
}

/// Represents an arbitrary HTTPRoute filter while preserving its nested fields in Kubernetes.
struct McpBackendFilterSchema;

impl schemars::JsonSchema for McpBackendFilterSchema {
    fn schema_name() -> String {
        "McpBackendFilter".to_string()
    }

    fn json_schema(_generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::SingleOrVec::Single(Box::new(schemars::schema::InstanceType::Object))),
            ..Default::default()
        };
        schema.extensions.insert("x-kubernetes-preserve-unknown-fields".to_string(), serde_json::Value::Bool(true));
        schemars::schema::Schema::Object(schema)
    }
}

/// Builds a structural CRD schema without expanding HttpRouteFilter's complex enums.
fn mcp_backend_refs_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    <Vec<McpBackendRefSchema> as schemars::JsonSchema>::json_schema(generator)
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
