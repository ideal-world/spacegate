use crate::constants;
use crate::model::vo::Vo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use tardis::web::poem_openapi;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are four levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rule
/// 4. Backend level, which works on all requests under the same backend
#[derive(Default, Debug, Eq, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct SgFilterVo {
    /// unique by id
    pub id: String,
    /// Filter code, Used to match the corresponding filter.
    pub code: String,
    pub enable: bool,
    /// Filter name. If the name of the same filter exists at different levels of configuration,
    /// only the child nodes take effect（Backend Level > Rule Level > Routing Level > Global Level）
    pub name: Option<String>,
    /// filter parameters.
    pub spec: Value,
}

impl Hash for SgFilterVo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.code.hash(state);
        self.name.hash(state);
    }
}

impl PartialEq for SgFilterVo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Vo for SgFilterVo {
    fn get_vo_type() -> String {
        constants::PLUGIN_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        self.id.clone()
    }
}
