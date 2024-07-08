use std::{fmt::Display, hash::Hash};

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

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Eq)]
#[serde(rename_all = "camelCase")]
pub struct K8sSgFilterSpecTargetRef {
    /// # FilterTarget Kind
    ///  can be:
    /// - gateway
    /// - httproute
    /// - httpspaceroute
    /// - HttpspacerouteRule
    /// - HttpspacerouteBackend
    pub kind: String,
    pub name: String,
    /// if namespace is None, use SgFilter's namespace
    pub namespace: Option<String>,
}

impl Hash for K8sSgFilterSpecTargetRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.name.hash(state);
        self.namespace.as_deref().unwrap_or("").hash(state);
    }
}

impl PartialEq for K8sSgFilterSpecTargetRef {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.kind == other.kind && self.namespace.as_ref().unwrap_or(&"".to_string()) == other.namespace.as_ref().unwrap_or(&"".to_string())
    }
}

impl Display for K8sSgFilterSpecTargetRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ns) = self.namespace.clone() {
            write!(f, "{}:{}.{}", self.kind, self.name, ns)
        } else {
            write!(f, "{}:{}", self.kind, self.name)
        }
    }
}
