use std::sync::Arc;

use hyper::{header::HeaderName, Request, Response};
use serde::{Deserialize, Serialize};

use spacegate_ext_redis::{global_repo, redis::Script};
use spacegate_kernel::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::function::{FnLayer, FnLayerMethod, Inner},
    BoxError, BoxResult, SgBody, SgBoxLayer,
};

use crate::{error::code, MakeSgLayer, Plugin, PluginError};
use spacegate_kernel::ret_error;

use super::redis_format_key;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisLimitConfig {
    pub id: Option<String>,
    pub header: String,
}

pub struct RedisLimit {
    pub prefix: String,
    pub header: HeaderName,
    pub script: Script,
}

impl FnLayerMethod for RedisLimit {
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
        let Some(gateway_name) = req.extensions().get::<GatewayName>() else {
            return PluginError::internal_error::<RedisLimitPlugin>("missing gateway name").into();
        };
        let Some(client) = global_repo().get(gateway_name) else {
            return PluginError::internal_error::<RedisLimitPlugin>("missing redis client").into();
        };
        let mut conn = client.get_conn().await;
        let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
            return PluginError::internal_error::<RedisLimitPlugin>("missing matched router").into();
        };
        let Some(key) = redis_format_key(&req, matched, &self.header) else {
            return PluginError::status::<RedisLimitPlugin, { code::UNAUTHORIZED }>(format!("missing header {}", self.header.as_str())).into();
        };
        let key = format!("{}:{}", self.prefix, key);
        let pass: bool = ret_error!(self.script.key(key).invoke_async(&mut conn).await.map_err(PluginError::internal_error::<RedisLimitPlugin>));
        if !pass {
            return PluginError::status::<RedisLimitPlugin, { code::TOO_MANY_REQUESTS }>("too many request, please try later").into();
        }
        inner.call(req).await
    }
}

impl MakeSgLayer for RedisLimitConfig {
    fn make_layer(&self) -> BoxResult<spacegate_kernel::SgBoxLayer> {
        let check_script = Script::new(include_str!("./redis_limit/check.lua"));
        let method = Arc::new(RedisLimit {
            prefix: RedisLimitPlugin::redis_prefix(self.id.as_deref()),
            header: HeaderName::from_bytes(self.header.as_bytes())?,
            script: check_script,
        });
        let layer = FnLayer::new(method);
        Ok(SgBoxLayer::new(layer))
    }
}

pub struct RedisLimitPlugin;
impl Plugin for RedisLimitPlugin {
    type MakeLayer = RedisLimitConfig;

    const CODE: &'static str = "redis-limit";

    fn create(id: Option<String>, value: serde_json::Value) -> Result<Self::MakeLayer, BoxError> {
        let config = serde_json::from_value::<RedisLimitConfig>(value)?;
        Ok(RedisLimitConfig {
            id: id.or(config.id),
            header: config.header,
        })
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisLimitPlugin);

#[cfg(test)]
mod test {
    use super::*;
    use crate::Plugin;
    use hyper::header::AUTHORIZATION;
    use hyper::service::HttpService;
    use serde_json::json;
    use spacegate_ext_redis::redis::AsyncCommands;
    use spacegate_kernel::{
        layers::http_route::match_request::{SgHttpPathMatch, SgHttpRouteMatch},
        service::get_echo_service,
    };
    use std::time::Duration;
    use testcontainers_modules::redis::REDIS_PORT;
    use tower_layer::Layer;
    #[tokio::test]
    async fn test_op_res_freq_limit() {
        const GW_NAME: &str = "DEFAULT";
        const AK: &str = "3qpm";
        std::env::set_var("RUST_LOG", "trace");

        let docker = testcontainers::clients::Cli::default();
        let redis_container = docker.run(testcontainers_modules::redis::Redis);
        let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

        let url = format!("redis://127.0.0.1:{host_port}");
        let config = RedisLimitPlugin::create(
            Some("test".into()),
            json! {
                {
                    "header": AUTHORIZATION.as_str(),
                }
            },
        )
        .expect("invalid config");
        global_repo().add(GW_NAME, url.as_str());
        let client = global_repo().get(GW_NAME).expect("missing client");
        let mut conn = client.get_conn().await;
        let _: () = conn.set(format!("sg:plugin:redis-limit:test:*:op-res:{AK}"), 3).await.expect("fail to set");
        let layer = config.make_layer().expect("fail to make layer");
        let backend_service = get_echo_service();
        let mut service = layer.layer(backend_service);
        {
            fn gen_req(ak: &str) -> Request<SgBody> {
                Request::builder()
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
                    .header(AUTHORIZATION, ak.to_string())
                    .body(SgBody::empty())
                    .expect("fail to build")
            }
            for _times in 0..3 {
                let resp = service.call(gen_req(AK)).await.expect("infallible");
                let (parts, body) = resp.into_parts();
                let body = body.dump().await.expect("fail to dump");
                println!("body: {body:?}, parts: {parts:?}");
                assert!(parts.status.is_success());
            }
            let resp = service.call(gen_req(AK)).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            let body = body.dump().await.expect("fail to dump");
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_client_error());
            tokio::time::sleep(Duration::from_secs(61)).await;
            let resp = service.call(gen_req(AK)).await.expect("infallible");
            let (parts, body) = resp.into_parts();
            let body = body.dump().await.expect("fail to dump");
            println!("body: {body:?}, parts: {parts:?}");
            assert!(parts.status.is_success());
        }
    }
}
