use std::{str::FromStr, sync::Arc};

use super::redis_format_key;
use crate::{
    def_plugin,
    model::{SgHttpPathModifier, SgHttpPathModifierType},
    MakeSgLayer, PluginError,
};
use futures_util::future::BoxFuture;
use hyper::http;
use hyper::{header::HeaderName, Uri};
use serde::{Deserialize, Serialize};
use spacegate_ext_redis::redis::AsyncCommands;
use spacegate_kernel::{
    extension::MatchedSgRouter,
    helper_layers::async_filter::{AsyncFilter, AsyncFilterRequestLayer},
    layers::http_route::match_request::SgHttpPathMatch,
    ReqOrResp, SgBody, SgBoxLayer, SgRequestExt,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisDynamicRouteConfig {
    pub prefix: String,
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
            let Some(SgHttpPathMatch::Prefix(prefix_match)) = &matched.path else {
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
            .replace(path, Some(prefix_match.as_str()))
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

impl MakeSgLayer for RedisDynamicRouteConfig {
    fn make_layer(&self) -> spacegate_kernel::BoxResult<SgBoxLayer> {
        Ok(SgBoxLayer::new(AsyncFilterRequestLayer::new(RedisDynamicRoute {
            prefix: self.prefix.clone().into(),
            header: HeaderName::from_str(&self.header)?.into(),
        })))
    }
}

def_plugin!("redis-dynamic-route", RedisDynamicRoutePlugin, RedisDynamicRouteConfig);
