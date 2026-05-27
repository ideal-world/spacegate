use std::time::Duration;

use hyper::{Request, Response, Uri};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::RedisClient;
use spacegate_kernel::{helper_layers::function::Inner, BoxError, SgBody};

use crate::Plugin;

use super::{
    types::{HaiApiIdentity, HaiAuditState, HaiDispatch},
    util,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HaiDispatchConfig {
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default)]
    pub gateway_hosts: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_asset_self_lookup: bool,
}

fn default_true() -> bool {
    true
}

pub struct HaiDispatchPlugin {
    config: HaiDispatchConfig,
    redis: Option<RedisClient>,
}

impl HaiDispatchPlugin {
    fn error(status: hyper::StatusCode, code: &str, message: &str) -> Response<SgBody> {
        Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(SgBody::full(serde_json::json!({ "code": code, "message": message }).to_string()))
            .expect("valid response")
    }
}

impl Plugin for HaiDispatchPlugin {
    const CODE: &'static str = "hai-dispatch";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<HaiDispatchConfig>(config.spec)?;
        let redis = util::redis_client_from_url(config.redis_url.as_deref())?;
        Ok(Self { config, redis })
    }

    async fn call(&self, mut req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let Some(asset) = util::current_asset_with_client(&req, self.redis.as_ref(), self.config.allow_asset_self_lookup).await? else {
            return Ok(Self::error(hyper::StatusCode::INTERNAL_SERVER_ERROR, "missing_asset_context", "missing asset context"));
        };

        req.headers_mut().insert("x-asset-id", asset.asset_id.parse()?);
        let caller_app_id = req.extensions().get::<HaiApiIdentity>().map(|identity| identity.0.app_id.clone());
        if let Some(caller_app_id) = caller_app_id {
            req.headers_mut().insert("x-caller-app-id", caller_app_id.parse()?);
        }

        if let Some(content) = &asset.asset_content {
            return Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "text/plain")
                .body(SgBody::full(content.clone()))
                .expect("valid response"));
        }
        if let Some(url) = &asset.asset_url {
            return Ok(Response::builder()
                .status(hyper::StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .body(SgBody::full(serde_json::json!({ "url": url }).to_string()))
                .expect("valid response"));
        }

        let Some(endpoint) = asset.runtime_endpoint.clone() else {
            return Ok(Self::error(hyper::StatusCode::NOT_FOUND, "asset_not_found", "asset runtime endpoint missing"));
        };
        let endpoint = util::inject_asset_secrets(&mut req, &asset, &endpoint)?;
        let uri: Uri = endpoint.parse()?;
        let protocol = if util::is_mcp_path(req.uri().path()) || asset.asset_type == "mcp" {
            "mcp"
        } else if asset.asset_type == "model" {
            "openai"
        } else {
            "passthrough"
        }
        .to_string();
        let is_streaming = protocol == "mcp";
        let dispatch = HaiDispatch {
            protocol,
            upstream: Some(uri.clone()),
            timeout_ms: asset.timeout_sec.map(|sec| sec * 1000),
            is_model: asset.asset_type == "model",
            is_streaming,
        };
        if let Some(audit) = req.extensions().get::<HaiAuditState>() {
            audit.update(|data| data.dispatch = Some(dispatch.clone()));
        }
        req.extensions_mut().insert(dispatch);
        *req.uri_mut() = uri;

        if let Some(timeout_sec) = asset.timeout_sec {
            let fut = inner.call(req);
            match tokio::time::timeout(Duration::from_secs(timeout_sec), fut).await {
                Ok(resp) => Ok(resp),
                Err(_) => Ok(Self::error(hyper::StatusCode::GATEWAY_TIMEOUT, "upstream_timeout", "upstream timeout")),
            }
        } else {
            Ok(inner.call(req).await)
        }
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(HaiDispatchPlugin, HaiDispatchConfig);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spacegate_model::PluginInstanceName;

    #[test]
    fn create_caches_plugin_redis_client_when_redis_url_is_configured() {
        let plugin = HaiDispatchPlugin::create_by_spec(
            json!({
                "redis_url": "redis://127.0.0.1:6379",
                "allow_asset_self_lookup": true
            }),
            PluginInstanceName::named("test"),
        )
        .expect("valid config");

        assert!(plugin.redis.is_some());
    }
}
