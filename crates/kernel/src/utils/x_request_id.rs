use std::{
    hash::Hasher,
    sync::{atomic::AtomicU64, OnceLock},
};

use hyper::{http::HeaderValue, Request, Response};

use crate::{helper_layers::function::Inner, SgBody};

pub trait XRequestIdAlgo {
    fn generate() -> HeaderValue;
}
pub const X_REQUEST_ID_HEADER_NAME: &str = "x-request-id";
/// Add a `x-request-id` header to the request and then response.
///
/// If the request already has a `x-request-id` header, it will be used.
pub async fn x_request_id<A: XRequestIdAlgo>(mut request: Request<SgBody>, inner: Inner) -> Response<SgBody> {
    let id = if let Some(id) = request.headers().get(X_REQUEST_ID_HEADER_NAME) {
        id.clone()
    } else {
        let id = A::generate();
        request.headers_mut().insert(X_REQUEST_ID_HEADER_NAME, id.clone());
        id
    };
    let mut resp = inner.call(request).await;
    resp.headers_mut().insert(X_REQUEST_ID_HEADER_NAME, id);
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

fn machine_id() -> u64 {
    static MACHINE_ID: OnceLock<u64> = OnceLock::new();
    *MACHINE_ID.get_or_init(|| {
        let mid = std::env::var("MACHINE_ID");
        let mut hasher = std::hash::DefaultHasher::new();
        if let Ok(mid) = mid {
            if let Ok(mid) = mid.parse::<u64>() {
                mid
            } else {
                hasher.write(mid.as_bytes());
                hasher.finish()
            }
        } else {
            #[cfg(target_os = "linux")]
            {
                // let's try to read system mid
                let mid = std::fs::read_to_string("/var/lib/dbus/machine-id").expect("fail to read machine id");
                hasher.write(mid.as_bytes());
                hasher.finish()
            }
            #[cfg(not(target_os = "linux"))]
            {
                // let's generate random one
                let mid = rand::random::<u64>();
            }
        }
    })
}
