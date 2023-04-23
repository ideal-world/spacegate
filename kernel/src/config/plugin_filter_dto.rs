use serde::{Deserialize, Serialize};

use crate::plugins::filters::{header_modifier::SgFilerHeaderModifier, redirect::SgFilerRedirect};

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
///
/// There are three levels of filters
/// 1. Global level, which works on all requests under the same gateway service
/// 2. Routing level, which works on all requests under the same gateway route
/// 3. Rule level, which works on all requests under the same gateway routing rules
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// Filter name, the name of the same filter exists at different levels of configuration, only the child nodes take effect（Rule Level > Routing Level > Global Level）
    pub name: Option<String>,
    /// HeaderModifier defines a schema for a header modifier filter.
    pub header_modifier: Option<SgFilerHeaderModifier>,
    /// Redirect defines a schema for a redirect filter.
    pub redirect: Option<SgFilerRedirect>,
}
