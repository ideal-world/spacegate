use serde::{Deserialize, Serialize};
use serde_json::Value;
use spacegate_tower::{BoxError, SgBoxLayer};

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

impl SgRouteFilter {
    pub fn into_layer(self) -> Result<SgBoxLayer, BoxError> {
        let plugin_repo = spacegate_plugin::SgPluginRepository::global();
        plugin_repo.create(&self.code, self.spec)?.make_layer()
    }
}
