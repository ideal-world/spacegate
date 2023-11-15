use crate::inner_model::plugin_filter::SgHttpPathModifier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const SG_FILTER_HEADER_MODIFIER_CODE: &str = "header_modifier";

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterHeaderModifier {
    pub kind: SgFilterHeaderModifierKind,
    pub sets: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub enum SgFilterHeaderModifierKind {
    #[default]
    Request,
    Response,
}

pub const SG_FILTER_REDIRECT_CODE: &str = "redirect";

/// RedirectFilter defines a filter that redirects a request.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterRedirect {
    /// Scheme is the scheme to be used in the value of the Location header in the response. When empty, the scheme of the request is used.
    pub scheme: Option<String>,
    /// Hostname is the hostname to be used in the value of the Location header in the response. When empty, the hostname in the Host header of the request is used.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
    /// Port is the port to be used in the value of the Location header in the response.
    pub port: Option<u16>,
    /// StatusCode is the HTTP status code to be used in response.
    pub status_code: Option<u16>,
}

pub const SG_FILTER_REWRITE_CODE: &str = "rewrite";

/// RewriteFilter defines a filter that modifies a request during forwarding.
///
/// https://gateway-api.sigs.k8s.io/geps/gep-726/
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterRewrite {
    /// Hostname is the value to be used to replace the Host header value during forwarding.
    pub hostname: Option<String>,
    /// Path defines parameters used to modify the path of the incoming request. The modified path is then used to construct the Location header. When empty, the request path is used as-is.
    pub path: Option<SgHttpPathModifier>,
}
