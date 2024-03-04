use serde::{Deserialize, Serialize};

use k8s_openapi::schemars::JsonSchema;
use kube::CustomResource;

use serde_json::value::Value;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[kube(kind = "SgFilter", group = "spacegate.idealworld.group", version = "v1", namespaced)]
pub struct K8sSgFilterSpec {
    pub filters: Vec<K8sSgFilterSpecFilter>,
    pub target_refs: Vec<K8sSgFilterSpecTargetRef>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct K8sSgFilterSpecFilter {
    /// see [crate::inner_model::plugin_filter::SgRouteFilter].code
    pub code: String,
    pub name: Option<String>,
    pub enable: bool,
    pub config: Value,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct K8sSgFilterSpecTargetRef {
    /// # FilterTarget Kind
    ///  can be:
    /// - gateway
    /// - httproute
    /// - httpspaceroute
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
}

impl PartialEq for K8sSgFilterSpecTargetRef {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.kind == other.kind && self.namespace.as_ref().unwrap_or(&"".to_string()) == other.namespace.as_ref().unwrap_or(&"".to_string())
    }
}

pub enum SgFilterTargetKind {
    Gateway,
    Httproute,
    Httpspaceroute,
}

impl From<SgFilterTargetKind> for String {
    fn from(value: SgFilterTargetKind) -> Self {
        match value {
            SgFilterTargetKind::Gateway => "gateway".to_string(),
            SgFilterTargetKind::Httproute => "httproute".to_string(),
            SgFilterTargetKind::Httpspaceroute => "httpspaceroute".to_string(),
        }
    }
}
