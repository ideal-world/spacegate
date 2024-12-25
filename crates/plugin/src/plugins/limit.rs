use std::{
    net::IpAddr,
    sync::{Arc, OnceLock},
    time::SystemTime,
};

use hyper::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};

use serde_json::Value;
use spacegate_kernel::{extension::OriginalIpAddr, helper_layers::function::Inner, BoxError, SgBody, SgRequestExt, SgResponseExt};

use crate::Plugin;
use spacegate_ext_redis::redis::Script;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimitPluginConfig {
    /// Maximum number of requests, default is 100
    pub max_request_number: Option<u64>,
    /// Time window in milliseconds, default is 1000ms
    pub time_window_ms: Option<u64>,

    pub report_ext: Value,
}

#[derive(Debug, Clone)]
pub struct RateLimitPlugin {
    pub max_request_number: u64,
    pub time_window_ms: u64,
    pub report_ext: Arc<Value>,
    pub id: Arc<str>,
}

impl RateLimitPlugin {
    pub fn report(&self, rising_edge: bool, original_ip_addr: IpAddr) -> RateLimitReport {
        RateLimitReport {
            rising_edge,
            original_ip_addr,
            plugin: self.clone(),
        }
    }
}

const DEFAULT_TIME_WINDOW_MS: u64 = 1000;
const DEFAULT_MAX_REQUEST_NUMBER: u64 = 100;
const CONF_LIMIT_KEY: &str = "sg:plugin:filter:limit:";

#[derive(Debug, Clone)]
pub struct RateLimitReport {
    pub rising_edge: bool,
    pub original_ip_addr: IpAddr,
    pub plugin: RateLimitPlugin,
}
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

impl RateLimitPlugin {}

impl Plugin for RateLimitPlugin {
    const CODE: &'static str = "limit";
    fn meta() -> spacegate_model::PluginMetaData {
        crate::plugin_meta!(
            description: "Request rate limit plugin."
        )
    }
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let id = &self.id;
        let ip = req.extract::<OriginalIpAddr>().to_canonical();
        let mut conn = req.get_redis_client_by_gateway_name().ok_or("missing gateway name")?.get_conn().await;

        const EXCEEDED: i32 = 0;
        const RISING_EDGE: i32 = 1;
        let result: i32 = script()
            // counter key
            .key(format!("{CONF_LIMIT_KEY}{id}:{ip}"))
            // last counter reset timestamp key
            .key(format!("{CONF_LIMIT_KEY}{id}:{ip}_ts"))
            // maximum number of request
            .arg(self.max_request_number)
            // time window
            .arg(self.time_window_ms)
            // current timestamp
            .arg(SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("invalid system time: before unix epoch").as_millis() as u64)
            .invoke_async(&mut conn)
            .await?;

        if result == EXCEEDED || result == RISING_EDGE {
            let mut response = Response::<SgBody>::with_code_message(StatusCode::TOO_MANY_REQUESTS, "[SG.Filter.Limit] too many requests");
            response.extensions_mut().insert(self.report(result == RISING_EDGE, ip));
            return Ok(response);
        }
        Ok(inner.call(req).await)
    }
    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let spec = serde_json::from_value::<RateLimitPluginConfig>(config.spec)?;
        let id = config.id.to_string();
        Ok(Self {
            max_request_number: spec.max_request_number.unwrap_or(DEFAULT_MAX_REQUEST_NUMBER),
            time_window_ms: spec.time_window_ms.unwrap_or(DEFAULT_TIME_WINDOW_MS),
            report_ext: Arc::new(spec.report_ext),
            id: Arc::from(id),
        })
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema! { RateLimitPlugin, RateLimitPluginConfig }
