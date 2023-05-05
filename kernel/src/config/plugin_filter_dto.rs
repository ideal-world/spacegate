use serde::{Deserialize, Serialize};
use serde_json::Value;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are four levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rule
/// 4. Backend level, which works on all requests under the same backend
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// Filter code, Used to match the corresponding filter.
    pub code: String,
    /// Filter name, the name of the same filter exists at different levels of configuration, only the child nodes take effect（Backend Level > Rule Level > Routing Level > Global Level）
    pub name: Option<String>,
    /// filter parameters.
    pub spec: Value,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpPathModifier {
    /// Type defines the type of path modifier.
    pub kind: SgHttpPathModifierType,
    /// Value is the value to be used to replace the path during forwarding.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SgHttpPathModifierType {
    /// This type of modifier indicates that the full path will be replaced by the specified value.
    ReplaceFullPath,
    /// This type of modifier indicates that any prefix path matches will be replaced by the substitution value.
    /// For example, a path with a prefix match of “/foo” and a ReplacePrefixMatch substitution of “/bar” will have the “/foo” prefix replaced with “/bar” in matching requests.
    #[default]
    ReplacePrefixMatch,
}
