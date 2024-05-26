use hyper::Request;

use crate::SgBody;
mod extension;
pub use extension::*;
use hyper::http;
/// a marker is some information that can be attached to a request and can be extracted from a request.
pub trait Extract: Sized + Send + Sync {
    fn extract(req: &Request<SgBody>) -> Self;
}

impl Extract for http::uri::Uri {
    fn extract(req: &Request<SgBody>) -> Self {
        req.uri().clone()
    }
}

impl Extract for http::method::Method {
    fn extract(req: &Request<SgBody>) -> Self {
        req.method().clone()
    }
}

#[cfg(feature = "ext-redis")]
impl Extract for Option<spacegate_ext_redis::RedisClient> {
    fn extract(req: &Request<SgBody>) -> Self {
        crate::SgRequestExt::get_redis_client_by_gateway_name(req)
    }
}
