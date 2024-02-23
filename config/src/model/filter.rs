use serde::{Deserialize, Serialize};
use serde_json::Value;

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are four levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rule
/// 4. Backend level, which works on all requests under the same backend
#[cfg_attr(feature = "typegen", derive(ts_rs::TS), ts(export, export_to = "../admin-client/src/model/"))]
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// Filter code, Used to match the corresponding filter.
    pub code: String,
    /// Filter name, the name of the same filter exists at different levels of configuration, only the child nodes take effect（Backend Level > Rule Level > Routing Level > Global Level）
    pub name: Option<String>,
    /// filter parameters.
    #[cfg_attr(feature = "typegen", ts(type = "any"))]
    pub spec: Value,
}
