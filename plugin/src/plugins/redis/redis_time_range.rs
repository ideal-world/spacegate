use hyper::{header::HeaderName, Request, Response};
use serde::{Deserialize, Serialize};

use spacegate_ext_redis::{
    global_repo,
    redis::{AsyncCommands, RedisError},
    Connection,
};
use spacegate_kernel::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::function::Inner,
    BoxError, SgBody,
};
use tracing::debug;

use crate::{error::code, Plugin, PluginError};

use super::redis_format_key;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisTimeRangeConfig {
    pub id: Option<String>,
    pub header: String,
}

pub struct RedisTimeRangePlugin {
    pub prefix: String,
    pub header: HeaderName,
}

async fn redis_call(mut conn: Connection, time_key: String) -> Result<bool, RedisError> {
    let Result::<String, _>::Ok(time_range) = conn.get(&time_key).await else {
        debug!("fail to get time range with key {time_key}");
        return Ok(false);
    };
    let Some((from, to)) = time_range.split_once(',') else {
        debug!("fail to parse time range with key {time_key}, expect a ',' spliter");
        return Ok(false);
    };
    let (Ok(from), Ok(to)) = (chrono::DateTime::parse_from_rfc3339(from), chrono::DateTime::parse_from_rfc3339(to)) else {
        debug!("fail to parse time range ({from},{to}) with key {time_key}");
        return Ok(false);
    };
    let utc_now = chrono::Utc::now();
    Ok(from.to_utc() < utc_now && to.to_utc() >= utc_now)
}

impl Plugin for RedisTimeRangePlugin {
    // type MakeLayer = RedisTimeRangeConfig;

    const CODE: &'static str = "redis-time-range";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let instance_id = config.none_mono_id();
        let layer_config = serde_json::from_value::<RedisTimeRangeConfig>(config.spec.clone())?;
        Ok(RedisTimeRangePlugin {
            prefix: instance_id.redis_prefix(),
            header: HeaderName::from_bytes(layer_config.header.as_bytes())?,
        })
    }
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let _header = self.header.clone();
        let Some(gateway_name) = req.extensions().get::<GatewayName>() else {
            return Err("missing gateway name".into());
        };
        let Some(client) = global_repo().get(gateway_name) else {
            return Err("missing redis client".into());
        };
        let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
            return Err("missing matched router".into());
        };
        let Some(key) = redis_format_key(&req, matched, &self.header) else {
            return Ok(PluginError::status::<Self, { code::UNAUTHORIZED }>(format!("missing header {}", self.header.as_str())).into());
        };
        let pass: bool = redis_call(client.get_conn().await, format!("{}:{}", self.prefix, key)).await?;
        if !pass {
            return Ok(PluginError::status::<RedisTimeRangePlugin, { code::FORBIDDEN }>("request cumulative count reached the limit").into());
        }
        Ok(inner.call(req).await)
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisTimeRangePlugin, RedisTimeRangeConfig);

#[cfg(test)]
mod test {
    use super::*;
    use hyper::header::AUTHORIZATION;
    use serde_json::json;
    use spacegate_kernel::{
        layers::http_route::match_request::{SgHttpMethodMatch, SgHttpPathMatch, SgHttpRouteMatch},
        service::get_echo_service,
    };
    use testcontainers_modules::redis::REDIS_PORT;

    use tracing_subscriber::EnvFilter;
    #[tokio::test]
    async fn test_op_res_count_limit() {
        const GW_NAME: &str = "DEFAULT";
        std::env::set_var("RUST_LOG", "trace");
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();

        let docker = testcontainers::clients::Cli::default();
        let redis_container = docker.run(testcontainers_modules::redis::Redis);
        let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

        let url = format!("redis://127.0.0.1:{host_port}");
        let plugin = RedisTimeRangePlugin::create_by_spec(
            json! {
                {
                    "header": AUTHORIZATION.as_str(),
                }
            },
            Some("test".into()),
        )
        .expect("invalid config");
        global_repo().add(GW_NAME, url.as_str());
        let client = global_repo().get(GW_NAME).expect("missing client");
        let mut conn = client.get_conn().await;
        let _: () = conn
            .set(
                "sg:plugin:redis-time-range:test:*:op-res:ak-not-pass",
                "2025-01-01T00:00:00-08:00,2026-01-01T00:00:00-08:00",
            )
            .await
            .expect("fail to set");
        let _: () = conn.set("sg:plugin:redis-time-range:test:*:op-res:ak-pass", "2024-01-01T00:00:00-08:00,2025-01-01T00:00:00-08:00").await.expect("fail to set");
        let inner = Inner::new(get_echo_service());
        {
            let req = Request::builder()
                .uri("http://127.0.0.1/op-res/example")
                .method("GET")
                .extension(GatewayName::new(GW_NAME))
                .extension(MatchedSgRouter(
                    SgHttpRouteMatch {
                        path: Some(SgHttpPathMatch::Prefix("op-res".to_string())),
                        ..Default::default()
                    }
                    .into(),
                ))
                .header(AUTHORIZATION, "ak-pass")
                .body(SgBody::empty())
                .expect("fail to build");
            let resp = plugin.call(req, inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            let body = body.dump().await.expect("fail to dump");
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_success());
        }
        {
            let req = Request::builder()
                .uri("http://127.0.0.1/op-res/example")
                .method("GET")
                .extension(GatewayName::new(GW_NAME))
                .extension(MatchedSgRouter(
                    SgHttpRouteMatch {
                        path: Some(SgHttpPathMatch::Prefix("op-res".to_string())),
                        ..Default::default()
                    }
                    .into(),
                ))
                .header(AUTHORIZATION, "ak-not-pass")
                .body(SgBody::empty())
                .expect("fail to build");
            let resp = plugin.call(req, inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_client_error());
        }
        {
            let req = Request::builder()
                .uri("http://127.0.0.1/op-res/example")
                .method("POST")
                .extension(GatewayName::new(GW_NAME))
                .extension(MatchedSgRouter(
                    SgHttpRouteMatch {
                        path: Some(SgHttpPathMatch::Prefix("op-res".to_string())),
                        ..Default::default()
                    }
                    .into(),
                ))
                .body(SgBody::empty())
                .expect("fail to build");
            let resp = plugin.call(req, inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_client_error());
        }
        {
            let req = Request::builder()
                .uri("http://127.0.0.1/op-res/example")
                .method("DELETE")
                .extension(GatewayName::new(GW_NAME))
                .extension(MatchedSgRouter(
                    SgHttpRouteMatch {
                        path: Some(SgHttpPathMatch::Prefix("op-res".to_string())),
                        method: Some(vec![SgHttpMethodMatch("DELETE".into())]),
                        ..Default::default()
                    }
                    .into(),
                ))
                .header(AUTHORIZATION, "ak-pass")
                .body(SgBody::empty())
                .expect("fail to build");
            let resp = plugin.call(req, inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_client_error());
        }
    }
}
