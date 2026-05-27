use chrono::Utc;
use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::redis::AsyncCommands as _;
use spacegate_ext_redis::RedisClient;
use spacegate_kernel::{extension::PeerAddr, helper_layers::function::Inner, BoxError, SgBody};

use crate::Plugin;

use super::{
    types::{ApiKeyRecord, HaiApiIdentity, HaiAuditState, HaiRequestContext},
    util,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HaiAuthConfig {
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
}

pub struct HaiAuthPlugin {
    config: HaiAuthConfig,
    redis: Option<RedisClient>,
}

impl HaiAuthPlugin {
    fn error(status: hyper::StatusCode, code: &str, message: &str) -> Response<SgBody> {
        Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(SgBody::full(serde_json::json!({ "code": code, "message": message }).to_string()))
            .expect("valid response")
    }

    fn validate(record: &ApiKeyRecord, asset_id: &str, client_ip: &str, client_mac: &str) -> Result<(), Response<SgBody>> {
        if record.expired_at <= Utc::now() {
            return Err(Self::error(hyper::StatusCode::UNAUTHORIZED, "invalid_api_key", "api key expired"));
        }
        if !util::check_addr(client_ip, &record.allow_ips, &record.deny_ips) {
            return Err(Self::error(hyper::StatusCode::UNAUTHORIZED, "addr_forbidden", "client ip forbidden"));
        }
        if !record.deny_mac_addrs.is_empty() && record.deny_mac_addrs.iter().any(|mac| mac.eq_ignore_ascii_case(client_mac)) {
            return Err(Self::error(hyper::StatusCode::UNAUTHORIZED, "addr_forbidden", "client mac forbidden"));
        }
        if !record.allow_mac_addrs.is_empty() && !record.allow_mac_addrs.iter().any(|mac| mac.eq_ignore_ascii_case(client_mac)) {
            return Err(Self::error(hyper::StatusCode::UNAUTHORIZED, "addr_forbidden", "client mac forbidden"));
        }
        if !record.asset_ids.iter().any(|allowed| allowed == asset_id) {
            return Err(Self::error(hyper::StatusCode::FORBIDDEN, "not_subscribed", "asset not subscribed"));
        }
        Ok(())
    }
}

impl Plugin for HaiAuthPlugin {
    const CODE: &'static str = "hai-auth";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<HaiAuthConfig>(config.spec)?;
        let redis = util::redis_client_from_url(config.redis_url.as_deref())?;
        Ok(Self { config, redis })
    }

    async fn call(&self, mut req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let path = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or_else(|| req.uri().path()).to_string();
        let Some(asset_id) = util::parse_path(&path) else {
            return Ok(Self::error(hyper::StatusCode::NOT_FOUND, "asset_not_found", "asset id missing"));
        };
        let api_key = util::header_str(&req, "hai-api-key").map(str::to_string).or_else(|| util::extract_bearer_token(util::header_str(&req, "authorization"))).unwrap_or_default();
        if api_key.is_empty() {
            return Ok(Self::error(hyper::StatusCode::UNAUTHORIZED, "missing_api_key", "missing api key"));
        }
        let asset_version = util::normalize_optional_header_value(util::header_str(&req, util::HAI_ASSET_VERSION_HEADER));
        let peer = req.extensions().get::<PeerAddr>().map(|p| p.0.to_string()).unwrap_or_default();
        let client_ip = util::resolve_client_ip(
            &peer,
            util::header_str(&req, "x-forwarded-for"),
            util::header_str(&req, "x-real-ip"),
            &self.config.trusted_proxy_cidrs,
        );
        let client_mac = util::header_str(&req, "hai-mac-addr").unwrap_or_default().to_string();
        let request_id = util::request_id(&req);
        let api_key_hash = util::hash_api_key(&api_key);

        let client = util::redis_client_or_gateway(self.redis.as_ref(), &req)?;
        let mut conn = client.get_conn().await;
        let raw: Option<String> = conn.get(util::api_key_key(&api_key)).await?;
        let Some(raw) = raw else {
            return Ok(Self::error(hyper::StatusCode::UNAUTHORIZED, "invalid_api_key", "invalid api key"));
        };
        let record: ApiKeyRecord = serde_json::from_str(&raw)?;
        if let Err(resp) = Self::validate(&record, &asset_id, &client_ip, &client_mac) {
            return Ok(resp);
        }

        let ctx = HaiRequestContext {
            asset_id,
            asset_version,
            api_key_hash,
            client_ip,
            client_mac,
            request_id,
        };
        if let Some(audit) = req.extensions().get::<HaiAuditState>() {
            audit.update(|data| {
                data.request = Some(ctx.clone());
                data.identity = Some(record.clone());
            });
        }
        req.extensions_mut().insert(ctx);
        req.extensions_mut().insert(HaiApiIdentity(record));
        Ok(inner.call(req).await)
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(HaiAuthPlugin, HaiAuthConfig);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spacegate_model::PluginInstanceName;

    #[test]
    fn create_caches_plugin_redis_client_when_redis_url_is_configured() {
        let plugin = HaiAuthPlugin::create_by_spec(
            json!({
                "redis_url": "redis://127.0.0.1:6379",
                "trusted_proxy_cidrs": ["10.0.0.0/8"]
            }),
            PluginInstanceName::named("test"),
        )
        .expect("valid config");

        assert!(plugin.redis.is_some());
    }
}
