use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, OnceLock},
    time::SystemTime,
};

use hyper::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};

use spacegate_kernel::{
    helper_layers::async_filter::{AsyncFilter, AsyncFilterRequest, AsyncFilterRequestLayer},
    SgBody, SgBoxLayer, SgRequestExt, SgResponseExt,
};

use crate::{def_plugin, MakeSgLayer, PluginError};
use spacegate_ext_redis::redis::Script;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimitConfig {
    pub max_request_number: Option<u64>,
    pub time_window_ms: Option<u64>,
    pub id: String,
}

const CONF_LIMIT_KEY: &str = "sg:plugin:filter:limit:";

/// Flow limit script
///
/// # Arguments
///
/// * KEYS[1]  counter key
/// * KEYS[2]  last counter reset timestamp key
/// * ARGV[1]  maximum number of request
/// * ARGV[2]  time window
/// * ARGV[3]  current timestamp
///
/// # Return
///
/// * 1   passed
/// * 0   limited
///
/// # Kernel logic
///
/// ```lua
/// -- Use `counter` to accumulate 1 for each request
/// local current_count = tonumber(redis.call('incr', KEYS[1]));
/// if current_count == 1 then
///     -- The current request is the first request, record the current timestamp
///     redis.call('set', KEYS[2], ARGV[3]);
/// end
/// -- When the `counter` value reaches the maximum number of requests
/// if current_count > tonumber(ARGV[1]) then
///     local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
///     if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
///          -- Last reset time + time window > current time,
///          -- indicating that the request has reached the upper limit within this time period,
///          -- so the request is limited
///         return 0;
///     end
///     -- Otherwise reset the counter and timestamp,
///     -- and allow the request
///     redis.call('set', KEYS[1], '1')
///     redis.call('set', KEYS[2], ARGV[3]);
/// end
/// return 1;
/// ```
pub fn script() -> &'static Script {
    static SCRIPT: OnceLock<Script> = OnceLock::new();
    SCRIPT.get_or_init(|| Script::new(include_str!("./limit/script.lua")))
}

impl RateLimitConfig {
    async fn req_filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        let id = &self.id;
        if let Some(max_request_number) = &self.max_request_number {
            let mut conn = req.get_redis_client_by_gateway_name().ok_or(PluginError::internal_error::<RateLimitPlugin>("missing gateway name"))?.get_conn().await;
            let result: &bool = &script()
                // counter key
                .key(format!("{CONF_LIMIT_KEY}{id}"))
                // last counter reset timestamp key
                .key(format!("{CONF_LIMIT_KEY}{id}_ts"))
                // maximum number of request
                .arg(max_request_number)
                // time window
                .arg(self.time_window_ms.unwrap_or(1000))
                // current timestamp
                .arg(SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("invalid system time: before unix epoch").as_millis() as u64)
                .invoke_async(&mut conn)
                .await
                .map_err(|e| Response::<SgBody>::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, format!("[SG.Filter.Limit] redis error: {e}")))?;

            if !result {
                return Err(Response::<SgBody>::with_code_message(StatusCode::TOO_MANY_REQUESTS, "[SG.Filter.Limit] too many requests"));
            }
        }
        Ok(req)
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitFilter {
    pub config: Arc<RateLimitConfig>,
}

impl AsyncFilter for RateLimitFilter {
    type Future = Pin<Box<dyn Future<Output = Result<Request<SgBody>, Response<SgBody>>> + Send + 'static>>;
    fn filter(&self, req: Request<SgBody>) -> Self::Future {
        let config = self.config.clone();
        Box::pin(async move { config.req_filter(req).await })
    }
}

pub type RateLimitLayer = AsyncFilterRequestLayer<RateLimitFilter>;
pub type RateLimit<S> = AsyncFilterRequest<RateLimitFilter, S>;

impl MakeSgLayer for RateLimitConfig {
    fn make_layer(&self) -> Result<SgBoxLayer, spacegate_kernel::BoxError> {
        let layer = RateLimitLayer::new(RateLimitFilter { config: Arc::new(self.clone()) });
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("limit", RateLimitPlugin, RateLimitConfig);
#[cfg(feature = "schema")]
crate::schema! { RateLimitPlugin }
