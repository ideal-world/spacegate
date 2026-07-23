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
    /// 插件实例的管理端展示名称，不进入运行时插件配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub enable: bool,
    pub config: Value,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema, Eq)]
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
    /// Execution priority for this target binding. Higher values run first.
    #[serde(default)]
    pub priority: i32,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::K8sSgFilterSpecTargetRef;

    #[test]
    fn plugin_binding_priority_round_trips_through_k8s_target_ref() {
        let legacy: K8sSgFilterSpecTargetRef = serde_json::from_value(json!({
            "kind": "Gateway",
            "name": "api",
            "namespace": "default"
        }))
        .unwrap();
        assert_eq!(legacy.priority, 0);

        let negative: K8sSgFilterSpecTargetRef = serde_json::from_value(json!({
            "kind": "HTTPRoute",
            "name": "chat",
            "priority": -50
        }))
        .unwrap();
        let value = serde_json::to_value(negative).unwrap();
        assert_eq!(value["priority"], -50);
    }
}
