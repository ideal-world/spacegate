use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::redis::Script;
use spacegate_ext_redis::RedisClient;
use spacegate_kernel::{helper_layers::function::Inner, BoxError, SgBody};

use crate::Plugin;

use super::util::{self, MissingAssetPolicy};

const ACQUIRE_LUA: &str = r#"
local qps_key = KEYS[1]
local concurrent_key = KEYS[2]
local max_concurrent = tonumber(ARGV[1])
local qps_limit = tonumber(ARGV[2])
local now_ms = tonumber(ARGV[3])

if max_concurrent >= 0 then
  local current = tonumber(redis.call('GET', concurrent_key) or '0')
  if current >= max_concurrent then
    return 2
  end
end

if qps_limit >= 0 then
  local window = math.floor(now_ms / 1000)
  local qps_count_key = qps_key .. ':' .. tostring(window)
  local count = tonumber(redis.call('INCR', qps_count_key))
  if count == 1 then
    redis.call('PEXPIRE', qps_count_key, 2000)
  end
  if count > qps_limit then
    return 1
  end
end

if max_concurrent >= 0 then
  redis.call('INCR', concurrent_key)
  redis.call('PEXPIRE', concurrent_key, 86400000)
end
return 0
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HaiQuotaConfig {
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default)]
    pub missing_asset_policy: MissingAssetPolicy,
    #[serde(default = "default_true")]
    pub allow_asset_self_lookup: bool,
}

fn default_true() -> bool {
    true
}

impl Default for HaiQuotaConfig {
    fn default() -> Self {
        Self {
            redis_url: None,
            missing_asset_policy: MissingAssetPolicy::Error,
            allow_asset_self_lookup: true,
        }
    }
}

pub struct HaiQuotaPlugin {
    config: HaiQuotaConfig,
    redis: Option<RedisClient>,
    acquire: Script,
}

impl HaiQuotaPlugin {
    fn error(status: hyper::StatusCode, code: &str, message: &str) -> Response<SgBody> {
        Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(SgBody::full(serde_json::json!({ "code": code, "message": message }).to_string()))
            .expect("valid response")
    }
}

impl Plugin for HaiQuotaPlugin {
    const CODE: &'static str = "hai-quota";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<HaiQuotaConfig>(config.spec)?;
        let redis = util::redis_client_from_url(config.redis_url.as_deref())?;
        Ok(Self {
            config,
            redis,
            acquire: Script::new(ACQUIRE_LUA),
        })
    }

    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let Some(asset) = util::current_asset_with_client(&req, self.redis.as_ref(), self.config.allow_asset_self_lookup).await? else {
            return match self.config.missing_asset_policy {
                MissingAssetPolicy::Skip => Ok(inner.call(req).await),
                MissingAssetPolicy::Error => Ok(Self::error(hyper::StatusCode::INTERNAL_SERVER_ERROR, "missing_asset_context", "missing asset context")),
            };
        };

        if asset.max_concurrent.is_none() && asset.qps_limit.is_none() {
            return Ok(inner.call(req).await);
        }

        let client = util::redis_client_or_gateway(self.redis.as_ref(), &req)?;
        let mut conn = client.get_conn().await;
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
        let result: i32 = self
            .acquire
            .key(util::quota_qps_key(&asset.asset_id))
            .key(util::quota_concurrent_key(&asset.asset_id))
            .arg(asset.max_concurrent.map(i64::from).unwrap_or(-1))
            .arg(asset.qps_limit.map(i64::from).unwrap_or(-1))
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await?;

        match result {
            0 => {
                let resp = inner.call(req).await;
                if asset.max_concurrent.is_some() {
                    use spacegate_ext_redis::redis::AsyncCommands as _;
                    let mut conn = client.get_conn().await;
                    let _: () = conn.decr(util::quota_concurrent_key(&asset.asset_id), 1).await.unwrap_or(());
                }
                Ok(resp)
            }
            1 => Ok(Self::error(hyper::StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded", "rate limit exceeded")),
            2 => Ok(Self::error(
                hyper::StatusCode::TOO_MANY_REQUESTS,
                "too_many_concurrent_requests",
                "too many concurrent requests",
            )),
            _ => Ok(Self::error(hyper::StatusCode::INTERNAL_SERVER_ERROR, "quota_error", "quota script returned invalid result")),
        }
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(HaiQuotaPlugin, HaiQuotaConfig);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spacegate_model::PluginInstanceName;

    #[test]
    fn create_caches_plugin_redis_client_when_redis_url_is_configured() {
        let plugin = HaiQuotaPlugin::create_by_spec(
            json!({
                "redis_url": "redis://127.0.0.1:6379",
                "missing_asset_policy": "error",
                "allow_asset_self_lookup": true
            }),
            PluginInstanceName::named("test"),
        )
        .expect("valid config");

        assert!(plugin.redis.is_some());
    }
}
