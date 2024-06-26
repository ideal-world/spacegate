use std::{str::FromStr, sync::Arc};

use super::redis_format_key;
use crate::{
    model::{SgHttpPathModifier, SgHttpPathModifierType},
    Plugin, PluginConfig, PluginError,
};

use hyper::http;
use hyper::{header::HeaderName, Uri};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::redis::AsyncCommands;
use spacegate_kernel::{extension::MatchedSgRouter, BoxError, SgBody, SgRequestExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisDynamicRouteConfig {
    pub id: Option<String>,
    pub header: String,
}

#[derive(Debug, Clone)]
pub struct RedisDynamicRoutePlugin {
    pub prefix: Arc<String>,
    pub header: Arc<HeaderName>,
}
impl Plugin for RedisDynamicRoutePlugin {
    const CODE: &'static str = "redis-dynamic-route";

    fn create(config: PluginConfig) -> Result<Self, BoxError> {
        let layer_config = serde_json::from_value::<RedisDynamicRouteConfig>(config.spec.clone())?;
        Ok(Self {
            prefix: config.id.redis_prefix().into(),
            header: HeaderName::from_str(&layer_config.header)?.into(),
        })
    }

    async fn call(&self, mut req: http::Request<SgBody>, inner: spacegate_kernel::helper_layers::function::Inner) -> Result<http::Response<SgBody>, BoxError> {
        let client = req.get_redis_client_by_gateway_name().ok_or("missing gateway name")?;
        let header = self.header.as_ref();
        let prefix = self.prefix.as_ref();
        let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
            return Err("unmatched request".into());
        };
        let Some(path_match) = &matched.path else {
            return Err("only prefix match was supported".into());
        };
        let Some(key) = redis_format_key(&req, matched, header) else {
            return Err(format!("missing header {}", header.as_str()).into());
        };
        let route_key = format!("{}:{}", prefix, key);
        let mut conn = client.get_conn().await;
        let domain: String = conn.get(route_key).await.map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?;
        let mut uri_parts = req.uri().clone().into_parts();
        let path = req.uri().path();
        let Some(mut new_pq) = SgHttpPathModifier {
            kind: SgHttpPathModifierType::ReplacePrefixMatch,
            value: "/".to_string(),
        }
        .replace(path, path_match) else {
            return Err("gateway internal error: fail to rewrite path.".into());
        };
        if let Some(query) = req.uri().query().filter(|q| !q.is_empty()) {
            new_pq.push('?');
            new_pq.push_str(query);
        }
        domain.split_once("://");
        if let Some((scheme_str, host_str)) = domain.split_once("://") {
            uri_parts.scheme = Some(http::uri::Scheme::from_str(scheme_str)?);
            uri_parts.authority = Some(http::uri::Authority::from_str(host_str)?);
        } else {
            return Err(format!("bad route domain {}", domain).into());
        }
        uri_parts.path_and_query = Some(http::uri::PathAndQuery::from_maybe_shared(new_pq)?);
        *req.uri_mut() = Uri::from_parts(uri_parts)?;
        Ok(inner.call(req).await)
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisDynamicRoutePlugin, RedisDynamicRouteConfig);
