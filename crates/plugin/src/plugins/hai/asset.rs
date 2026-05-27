use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::RedisClient;
use spacegate_kernel::{helper_layers::function::Inner, BoxError, SgBody};

use crate::Plugin;

use super::{
    types::{HaiAsset, HaiAuditState, HaiRequestContext},
    util,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HaiAssetConfig {
    #[serde(default)]
    pub redis_url: Option<String>,
    #[serde(default = "default_true")]
    pub allow_asset_self_lookup: bool,
}

fn default_true() -> bool {
    true
}

impl Default for HaiAssetConfig {
    fn default() -> Self {
        Self {
            redis_url: None,
            allow_asset_self_lookup: true,
        }
    }
}

pub struct HaiAssetPlugin {
    config: HaiAssetConfig,
    redis: Option<RedisClient>,
}

impl HaiAssetPlugin {
    fn error(status: hyper::StatusCode, code: &str, message: &str) -> Response<SgBody> {
        Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(SgBody::full(serde_json::json!({ "code": code, "message": message }).to_string()))
            .expect("valid response")
    }
}

impl Plugin for HaiAssetPlugin {
    const CODE: &'static str = "hai-asset";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<HaiAssetConfig>(config.spec)?;
        let redis = util::redis_client_from_url(config.redis_url.as_deref())?;
        Ok(Self { config, redis })
    }

    async fn call(&self, mut req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let path = req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or_else(|| req.uri().path()).to_string();
        let (asset_id, asset_version) = if let Some(ctx) = req.extensions().get::<HaiRequestContext>() {
            (ctx.asset_id.clone(), ctx.asset_version.clone())
        } else if self.config.allow_asset_self_lookup {
            let Some(asset_id) = util::parse_path(&path) else {
                return Ok(Self::error(hyper::StatusCode::NOT_FOUND, "asset_not_found", "asset id missing"));
            };
            let asset_version = util::normalize_optional_header_value(util::header_str(&req, util::HAI_ASSET_VERSION_HEADER));
            (asset_id, asset_version)
        } else {
            return Ok(Self::error(hyper::StatusCode::INTERNAL_SERVER_ERROR, "missing_request_context", "missing request context"));
        };

        let client = util::redis_client_or_gateway(self.redis.as_ref(), &req)?;
        let Some(asset) = util::load_asset_from_client(&client, &asset_id, asset_version.as_deref()).await? else {
            return Ok(Self::error(hyper::StatusCode::NOT_FOUND, "asset_not_found", "asset not found"));
        };
        if !util::asset_type_matches_path(&path, &asset.asset_type) {
            return Ok(Self::error(hyper::StatusCode::NOT_FOUND, "asset_not_found", "asset type mismatch"));
        }
        if asset.asset_status != "published" {
            return Ok(Self::error(hyper::StatusCode::FORBIDDEN, "asset_unavailable", "asset not published"));
        }
        if let Some(audit) = req.extensions().get::<HaiAuditState>() {
            audit.update(|data| data.asset = Some(asset.clone()));
        }
        req.extensions_mut().insert(HaiAsset(asset));
        Ok(inner.call(req).await)
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(HaiAssetPlugin, HaiAssetConfig);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spacegate_model::PluginInstanceName;

    #[test]
    fn create_caches_plugin_redis_client_when_redis_url_is_configured() {
        let plugin = HaiAssetPlugin::create_by_spec(
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
