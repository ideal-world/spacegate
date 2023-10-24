use serde::{Deserialize, Serialize};

use k8s_openapi::schemars::JsonSchema;
use kube::CustomResource;
use serde_json::Value;

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
