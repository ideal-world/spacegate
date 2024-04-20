use hyper::{header::HeaderName, Request, Response};
use serde::{Deserialize, Serialize};

use spacegate_ext_redis::{global_repo, redis::Script};
use spacegate_kernel::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::function::Inner,
    BoxError, SgBody,
};

use crate::{error::code, Plugin, PluginConfig, PluginError};

use super::redis_format_key;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisLimitConfig {
    pub id: Option<String>,
    pub header: String,
}

pub struct RedisLimitPlugin {
    pub prefix: String,
    pub header: HeaderName,
    pub script: Script,
}

impl Plugin for RedisLimitPlugin {
    const CODE: &'static str = "redis-limit";

    fn create(config: PluginConfig) -> Result<Self, BoxError> {
        let id = config.id;
        let layer_config = serde_json::from_value::<RedisLimitConfig>(config.spec)?;
        Ok(Self {
            prefix: id.redis_prefix(),
            header: HeaderName::from_bytes(layer_config.header.as_bytes())?,
            script: Script::new(include_str!("./redis_limit/check.lua")),
        })
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(<Self as crate::PluginSchemaExt>::schema())
    }
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let Some(gateway_name) = req.extensions().get::<GatewayName>() else {
            return Err("missing gateway name".into());
        };
        let Some(client) = global_repo().get(gateway_name) else {
            return Err("missing redis client".into());
        };
        let mut conn = client.get_conn().await;
        let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
            return Err("missing matched router".into());
        };
        let Some(key) = redis_format_key(&req, matched, &self.header) else {
            return Ok(PluginError::status::<RedisLimitPlugin, { code::UNAUTHORIZED }>(format!("missing header {}", self.header.as_str())).into());
        };
        let key = format!("{}:{}", self.prefix, key);
        let pass: bool = self.script.key(key).invoke_async(&mut conn).await?;
        if !pass {
            return Ok(PluginError::status::<RedisLimitPlugin, { code::TOO_MANY_REQUESTS }>("too many request, please try later").into());
        }
        Ok(inner.call(req).await)
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisLimitPlugin, RedisLimitConfig);

#[cfg(feature = "axum")]
pub mod axum_ext;
#[cfg(test)]
mod test {
    use super::*;
    use crate::Plugin;
    use hyper::header::AUTHORIZATION;
    use serde_json::json;
    use spacegate_ext_redis::redis::AsyncCommands;
    use spacegate_kernel::{
        service::http_route::match_request::{HttpPathMatchRewrite, HttpRouteMatch},
        backend_service::get_echo_service,
    };
    use std::time::Duration;
    use testcontainers_modules::redis::REDIS_PORT;

    #[tokio::test]
    async fn test_op_res_freq_limit() {
        const GW_NAME: &str = "DEFAULT";
        const AK: &str = "3qpm";
        std::env::set_var("RUST_LOG", "trace");

        let docker = testcontainers::clients::Cli::default();
        let redis_container = docker.run(testcontainers_modules::redis::Redis);
        let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

        let url = format!("redis://127.0.0.1:{host_port}");
        let plugin = RedisLimitPlugin::create_by_spec(
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
        let _: () = conn.set(format!("sg:plugin:redis-limit:test:*:op-res:{AK}"), 3).await.expect("fail to set");
        let inner = Inner::new(get_echo_service());
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
                    .header(AUTHORIZATION, ak.to_string())
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
            tokio::time::sleep(Duration::from_secs(61)).await;
            let resp = plugin.call(gen_req(AK), inner.clone()).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            let body = body.dump().await.expect("fail to dump");
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_success());
        }
    }
}
