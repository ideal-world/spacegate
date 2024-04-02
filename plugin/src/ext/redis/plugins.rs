use hyper::{header::HeaderName, Request};
use serde::{Deserialize, Serialize};
use spacegate_kernel::{extension::MatchedSgRouter, layers::http_route::match_request::SgHttpPathMatch, SgBody};

pub mod redis_count;
pub mod redis_dynamic_route;
pub mod redis_limit;
pub mod redis_time_range;

fn redis_format_key(req: &Request<SgBody>, matched: &MatchedSgRouter, header: &HeaderName) -> Option<String> {
    let is_method_any_match = matched.method.as_ref().is_none();
    let method = if !is_method_any_match { req.method().as_str() } else { "*" };
    let path = matched
        .path
        .as_ref()
        .map(|p| match p {
            SgHttpPathMatch::Exact(path) => path,
            SgHttpPathMatch::Prefix(path) => path,
            SgHttpPathMatch::Regular(regex) => regex.as_str(),
        })
        .unwrap_or("*");
    let header = req.headers().get(header).and_then(|v| v.to_str().ok())?;
    Some(format!("{}:{}:{}", method, path, header))
}

#[cfg(feature = "axum")]
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchSpecifier {
    /// None for Any
    pub method: Option<String>,
    /// None for Any
    pub path: Option<String>,
    pub header: String,
}
