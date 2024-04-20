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

use crate::{error::code, Plugin, PluginError};

use super::redis_format_key;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisCountConfig {
    pub id: Option<String>,
    pub header: String,
}

pub struct RedisCountPlugin {
    pub prefix: String,
    pub header: HeaderName,
}

async fn redis_call(mut conn: Connection, count_key: String) -> Result<bool, RedisError> {
    let count_cumulative_key = format!("{count_key}:cumulative-count");
    if !conn.exists(&count_cumulative_key).await? {
        conn.set(&count_cumulative_key, 1).await?;
    } else {
        conn.incr(&count_cumulative_key, 1).await?;
    }
    let count_cumulative: u32 = conn.get(&count_cumulative_key).await?;
    let count: u32 = conn.get(&count_key).await.unwrap_or(u32::MAX);
    Ok(count_cumulative <= count)
}

async fn redis_call_on_resp(mut conn: Connection, count_key: String) -> Result<(), RedisError> {
    let count_cumulative_key = format!("{count_key}:cumulative-count");
    let _count_cumulative: u32 = conn.decr(&count_cumulative_key, 1).await?;
    Ok(())
}

// pub struct RedisCountPlugin;
impl Plugin for RedisCountPlugin {
    const CODE: &'static str = "redis-count";

    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let layer_config = serde_json::from_value::<RedisCountConfig>(config.spec.clone())?;
        let id = config.id.clone();
        Ok(Self {
            prefix: id.redis_prefix(),
            header: HeaderName::from_bytes(layer_config.header.as_bytes())?,
        })
    }
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
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
            return Ok(PluginError::status::<RedisCountPlugin, { code::UNAUTHORIZED }>(format!("missing header {}", self.header.as_str())).into());
        };
        let pass: bool = redis_call(client.get_conn().await, format!("{}:{}", self.prefix, key)).await?;
        if !pass {
            return Ok(PluginError::status::<RedisCountPlugin, { code::FORBIDDEN }>("request cumulative count reached the limit").into());
        }
        let resp = inner.call(req).await;
        if resp.status().is_server_error() || resp.status().is_client_error() {
            if let Err(e) = redis_call_on_resp(client.get_conn().await, format!("{}:{}", self.prefix, key)).await {
                tracing::error!("redis execution error: {e}")
            }
        }
        Ok(resp)
    }
    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisCountPlugin, RedisCountConfig);

#[cfg(test)]
mod test {

    use super::*;
    use hyper::header::AUTHORIZATION;
    use serde_json::json;
    use spacegate_kernel::{
        service::http_route::match_request::{HttpPathMatchRewrite, HttpRouteMatch},
        backend_service::get_echo_service,
    };
    use testcontainers_modules::redis::REDIS_PORT;

    #[tokio::test]
    async fn test_op_res_count_limit() {
        const GW_NAME: &str = "DEFAULT";
        const AK: &str = "3count";
        std::env::set_var("RUST_LOG", "trace");

        let docker = testcontainers::clients::Cli::default();
        let redis_container = docker.run(testcontainers_modules::redis::Redis);
        let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

        let url = format!("redis://127.0.0.1:{host_port}");
        let plugin = RedisCountPlugin::create_by_spec(
            json! {
                {
                    "header": AUTHORIZATION.as_str(),
                }
            },
            spacegate_model::PluginInstanceName::named("test"),
        )
        .expect("invalid config");
        global_repo().add(GW_NAME, url.as_str());
        let client = global_repo().get(GW_NAME).expect("missing client");
        let mut conn = client.get_conn().await;
        let _: () = conn.set(format!("sg:plugin:redis-count:test:*:op-res:{AK}"), 3).await.expect("fail to set");
        let inner = Inner::new(get_echo_service());
        let _backend_service = get_echo_service();
        {
            fn gen_req(ak: &str) -> Request<SgBody> {
                Request::builder()
                    .uri("http://127.0.0.1/op-res/example")
                    .method("GET")
                    .extension(GatewayName::new(GW_NAME))
                    .extension(MatchedSgRouter(
                        HttpRouteMatch {
                            path: Some(HttpPathMatchRewrite::prefix("op-res")),
                            ..Default::default()
                        }
                        .into(),
                    ))
                    .header(AUTHORIZATION, ak)
                    .body(SgBody::empty())
                    .expect("fail to build")
            }
            for _times in 0..3 {
                let resp = plugin.call(gen_req(AK), inner.clone()).await.expect("infallible");
                let (parts, body) = resp.into_parts();
                let body = body.dump().await.expect("fail to dump");
                println!("body: {body:?}, parts: {parts:?}");
                assert!(parts.status.is_success());
            }
            let resp = plugin.call(gen_req(AK), inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            let body = body.dump().await.expect("fail to dump");
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_client_error());
        }
    }
}
