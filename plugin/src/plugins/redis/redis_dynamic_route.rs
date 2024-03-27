use std::{str::FromStr, sync::Arc};

use super::redis_format_key;
use crate::{
    instance::{PluginInstance, PluginInstanceId},
    model::{SgHttpPathModifier, SgHttpPathModifierType},
    Plugin, PluginConfig, PluginError,
};
use futures_util::future::BoxFuture;
use hyper::http;
use hyper::{header::HeaderName, Uri};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::redis::AsyncCommands;
use spacegate_kernel::{
    extension::MatchedSgRouter,
    helper_layers::async_filter::{AsyncFilter, AsyncFilterRequestLayer},
    BoxError, ReqOrResp, SgBody, SgBoxLayer, SgRequestExt,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisDynamicRouteConfig {
    pub id: Option<String>,
    pub header: String,
}

#[derive(Debug, Clone)]
pub struct RedisDynamicRoute {
    pub prefix: Arc<String>,
    pub header: Arc<HeaderName>,
}

impl AsyncFilter for RedisDynamicRoute {
    type Future = BoxFuture<'static, ReqOrResp>;

    fn filter(&self, mut req: http::Request<SgBody>) -> Self::Future {
        let prefix = self.prefix.clone();
        let header = self.header.clone();
        let task = async move {
            let client = req.get_redis_client_by_gateway_name().ok_or("missing gateway name").map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?;
            let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
                return Err(PluginError::internal_error::<RedisDynamicRoutePlugin>("unmatched request").into());
            };
            let Some(path_match) = &matched.path else {
                return Err(PluginError::internal_error::<RedisDynamicRoutePlugin>("only prefix match was supported").into());
            };
            let Some(key) = redis_format_key(&req, matched, &header) else {
                return Err(PluginError::internal_error::<RedisDynamicRoutePlugin>(format!("missing header {}", header.as_str())).into());
            };
            let route_key = format!("{}:{}", prefix, key);
            let mut conn = client.get_conn().await;
            let domain: String = conn.get(route_key).await.map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?;
            let mut uri_parts = req.uri().clone().into_parts();
            let path = req.uri().path();
            let mut new_pq = SgHttpPathModifier {
                kind: SgHttpPathModifierType::ReplacePrefixMatch,
                value: "/".to_string(),
            }
            .replace(path, path_match)
            .ok_or_else(|| PluginError::internal_error::<RedisDynamicRoutePlugin>("gateway internal error: fail to rewrite path."))?;
            if let Some(query) = req.uri().query().filter(|q| !q.is_empty()) {
                new_pq.push('?');
                new_pq.push_str(query);
            }
            uri_parts.authority = Some(http::uri::Authority::from_maybe_shared(domain).map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?);
            uri_parts.path_and_query = Some(http::uri::PathAndQuery::from_maybe_shared(new_pq).map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?);
            *req.uri_mut() = Uri::from_parts(uri_parts).map_err(PluginError::internal_error::<RedisDynamicRoutePlugin>)?;
            ReqOrResp::Ok(req)
        };
        Box::pin(task)
    }
}

pub struct RedisDynamicRoutePlugin;
impl Plugin for RedisDynamicRoutePlugin {
    const CODE: &'static str = "redis-dynamic-route";

    fn create(config: PluginConfig) -> Result<crate::instance::PluginInstance, BoxError> {
        let layer_config = serde_json::from_value::<RedisDynamicRouteConfig>(config.spec.clone())?;
        let make = move |instance: &PluginInstance| {
            let instance_id = instance.resource.get::<PluginInstanceId>().expect("missing instance id");
            Ok(SgBoxLayer::new(AsyncFilterRequestLayer::new(RedisDynamicRoute {
                prefix: instance_id.redis_prefix().into(),
                header: HeaderName::from_str(&layer_config.header)?.into(),
            })))
        };
        Ok(crate::instance::PluginInstance::new::<Self, _>(config, make))
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisDynamicRoutePlugin, RedisDynamicRouteConfig);
