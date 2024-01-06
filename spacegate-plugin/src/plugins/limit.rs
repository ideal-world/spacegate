use std::{future::Future, pin::Pin, sync::Arc, time::SystemTime};

use hyper::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use tardis::{cache::Script, tardis_static};

use spacegate_tower::{
    extension::GatewayName,
    helper_layers::async_filter::{AsyncFilter, AsyncFilterRequest, AsyncFilterRequestLayer},
    SgBody, SgBoxLayer, SgResponseExt,
};

use crate::{cache::Cache, def_plugin, MakeSgLayer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_request_number: Option<u64>,
    pub time_window_ms: Option<u64>,
    pub id: String,
}

const CONF_LIMIT_KEY: &str = "sg:plugin:filter:limit:";

tardis_static! {
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
    pub script: Script = Script::new(
        r"
    local current_count = tonumber(redis.call('incr', KEYS[1]));
    if current_count == 1 then
        redis.call('set', KEYS[2], ARGV[3]);
    end
    if current_count > tonumber(ARGV[1]) then
        local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
        if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
            return 0;
        end
        redis.call('set', KEYS[1], '1')
        redis.call('set', KEYS[2], ARGV[3]);
    end
    return 1;
    ",
    );
}

impl RateLimitConfig {
    async fn req_filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>> {
        let id = &self.id;
        if let Some(max_request_number) = &self.max_request_number {
            if let Some(gateway_name) = req.extensions().get::<GatewayName>() {
                let mut conn = Cache::get(gateway_name).await.map_err(Response::<SgBody>::internal_error)?.cmd().await.map_err(Response::<SgBody>::internal_error)?;
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
            } else {
                // missing context
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
    fn make_layer(&self) -> Result<SgBoxLayer, tower::BoxError> {
        let layer = RateLimitLayer::new(RateLimitFilter { config: Arc::new(self.clone()) });
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("limit", RateLimitPlugin, RateLimitConfig);
