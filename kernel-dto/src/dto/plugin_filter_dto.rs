use crate::k8s_crd::SgFilter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "admin-support")]
use tardis::web::poem_openapi;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are four levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rule
/// 4. Backend level, which works on all requests under the same backend
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "admin-support", derive(poem_openapi::Object))]
pub struct SgRouteFilter {
    /// Filter code, Used to match the corresponding filter.
    pub code: String,
    /// Filter name. If the name of the same filter exists at different levels of configuration,
    /// only the child nodes take effect（Backend Level > Rule Level > Routing Level > Global Level）
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
    /// This type of modifier indicates that the full path will be replaced by the
    /// specified value.
    ReplaceFullPath,
    /// This type of modifier indicates that any prefix path matches will be
    /// replaced by the substitution value.
    /// For example, a path with a prefix match of “/foo” and a ReplacePrefixMatch
    /// substitution of “/bar” will have the “/foo” prefix replaced with “/bar” in
    /// matching requests.
    #[default]
    ReplacePrefixMatch,
}

#[cfg(feature = "k8s")]
pub struct SgSingeFilter {
    pub name: Option<String>,
    pub namespace: String,
    pub filter: crate::k8s_crd::K8sSgFilterSpecFilter,
    pub target_ref: crate::k8s_crd::K8sSgFilterSpecTargetRef,
}

impl SgSingeFilter {
    pub fn to_sg_filter(self) -> SgFilter {
        crate::k8s_crd::SgFilter {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: self.name.clone(),
                namespace: Some(self.namespace.clone()),
                ..Default::default()
            },
            spec: crate::k8s_crd::K8sSgFilterSpec {
                filters: vec![self.filter],
                target_refs: vec![self.target_ref],
            },
        }
    }
}
