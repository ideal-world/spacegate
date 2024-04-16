use std::sync::atomic::AtomicU64;

use hyper::{http::HeaderValue, Request, Response};

use crate::{helper_layers::function::Inner, SgBody};

#[derive(Debug, Clone)]
pub struct RequestId(HeaderValue);

pub trait XRequestIdAlgo {
    fn generate() -> HeaderValue;
}

/// Add a `x-request-id` header to the request.
pub async fn x_request_id<A: XRequestIdAlgo>(mut request: Request<SgBody>, inner: Inner) -> Response<SgBody> {
    let id = A::generate();
    request.headers_mut().insert("x-request-id", id.clone());
    let mut resp = inner.call(request).await;
    resp.headers_mut().insert("x-request-id", id);
    resp
}
/// # Reference
/// - discord: https://discord.com/developers/docs/reference#snowflakes
/// - instagram: https://instagram-engineering.com/sharding-ids-at-instagram-1cf5a71e5a5c
/// # Bits
/// - 42: timestamp
/// - 10: machine id
/// - 12: increment id
pub struct Snowflake;

impl XRequestIdAlgo for Snowflake {
    fn generate() -> HeaderValue {
        static INC: AtomicU64 = AtomicU64::new(0);
        let ts_id = unsafe { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_unchecked().as_millis() as u64 } << 22;
        let mach_id = machine_id() << 12;
        let inc = INC.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = ts_id | mach_id | inc;
        unsafe { HeaderValue::from_str(&format!("{:016x}", id)).unwrap_unchecked() }
    }
}

pub fn machine_id() -> u64 {
    #[cfg(feature = "k8s")]
    {}
    #[cfg(not(feature = "k8s"))]
    {
        0
    }
}
