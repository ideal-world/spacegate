use serde::{Deserialize, Serialize};

use crate::plugins::filters::{header_modifier::SgFilerHeaderModifier, redirect::SgFilerRedirect};

/// RouteFilter defines processing steps that must be completed during the request or response lifecycle.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgRouteFilter {
    /// HeaderModifier defines a schema for a header modifier filter.
    pub header_modifier: Option<SgFilerHeaderModifier>,
    /// Redirect defines a schema for a redirect filter.
    pub redirect: Option<SgFilerRedirect>,
}
