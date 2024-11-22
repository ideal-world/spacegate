use hyper::Request;

use crate::SgBody;
mod extension;
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

pub trait OptionalExtract: Sized + Send + Sync {
    fn extract(req: &Request<SgBody>) -> Option<Self>;
}

impl<T> Extract for Option<T>
where
    T: OptionalExtract,
{
    fn extract(req: &Request<SgBody>) -> Option<T> {
        <T as OptionalExtract>::extract(req)
    }
}

#[cfg(feature = "ext-redis")]
impl OptionalExtract for spacegate_ext_redis::RedisClient {
    fn extract(req: &Request<SgBody>) -> Option<Self> {
        crate::SgRequestExt::get_redis_client_by_gateway_name(req)
    }
}
