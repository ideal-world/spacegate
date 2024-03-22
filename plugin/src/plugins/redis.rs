use hyper::{header::HeaderName, Request};
use spacegate_kernel::{extension::MatchedSgRouter, layers::http_route::match_request::SgHttpPathMatch, SgBody};

pub mod redis_count;
pub mod redis_dynamic_route;
pub mod redis_limit;
pub mod redis_time_range;

pub struct RedisRateLimitConfig {
    pub key: RedisRateLimitKey,
}

impl RedisRateLimitConfig {}

pub enum RedisRateLimitKey {
    Header(String),
}

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
