use std::sync::Arc;

use hyper::{header::HeaderName, Request, Response};
use serde::{Deserialize, Serialize};

use spacegate_ext_redis::{
    global_repo,
    redis::{AsyncCommands, RedisError},
    Connection,
};
use spacegate_kernel::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::function::{FnLayer, FnLayerMethod, Inner},
    BoxError, SgBody, SgBoxLayer,
};

use crate::{error::code, Plugin, PluginError};
use spacegate_kernel::ret_error;

use super::redis_format_key;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RedisCountConfig {
    pub id: Option<String>,
    pub header: String,
}

pub struct RedisCount {
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
    let count: u32 = conn.get(&count_key).await?;
    Ok(count_cumulative <= count)
}

async fn redis_call_on_resp(mut conn: Connection, count_key: String) -> Result<(), RedisError> {
    let count_cumulative_key = format!("{count_key}:cumulative-count");
    let _count_cumulative: u32 = conn.decr(&count_cumulative_key, 1).await?;
    Ok(())
}

impl FnLayerMethod for RedisCount {
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Response<SgBody> {
        let Some(gateway_name) = req.extensions().get::<GatewayName>() else {
            return PluginError::internal_error::<RedisCountPlugin>("missing gateway name").into();
        };
        let Some(client) = global_repo().get(gateway_name) else {
            return PluginError::internal_error::<RedisCountPlugin>("missing redis client").into();
        };
        let Some(matched) = req.extensions().get::<MatchedSgRouter>() else {
            return PluginError::internal_error::<RedisCountPlugin>("missing matched router").into();
        };
        let Some(key) = redis_format_key(&req, matched, &self.header) else {
            return PluginError::status::<RedisCountPlugin, { code::UNAUTHORIZED }>(format!("missing header {}", self.header.as_str())).into();
        };
        let pass: bool = ret_error!(redis_call(client.get_conn().await, format!("{}:{}", self.prefix, key)).await.map_err(PluginError::internal_error::<RedisCountPlugin>));
        if !pass {
            return PluginError::status::<RedisCountPlugin, { code::FORBIDDEN }>("request cumulative count reached the limit").into();
        }
        let resp = inner.call(req).await;
        if resp.status().is_server_error() || resp.status().is_client_error() {
            if let Err(e) = redis_call_on_resp(client.get_conn().await, format!("{}:{}", self.prefix, key)).await {
                tracing::error!("redis execution error: {e}")
            }
        }
        resp
    }
}

pub struct RedisCountPlugin;
impl Plugin for RedisCountPlugin {
    const CODE: &'static str = "redis-count";

    fn create(config: crate::PluginConfig) -> Result<crate::instance::PluginInstance, BoxError> {
        let instance_id = config.instance_id();

        let layer_config = serde_json::from_value::<RedisCountConfig>(config.spec.clone())?;
        let make = move || {
            let method = Arc::new(RedisCount {
                prefix: instance_id.redis_prefix(),
                header: HeaderName::from_bytes(layer_config.header.as_bytes())?,
            });
            let layer = FnLayer::new(method);
            Ok(SgBoxLayer::new(layer))
        };
        let instance = crate::instance::PluginInstance::new::<Self, _>(config, make);
        Ok(instance)
    }
}

#[cfg(feature = "schema")]
crate::schema!(RedisCountPlugin, RedisCountConfig);

#[cfg(test)]
mod test {

    use hyper::header::AUTHORIZATION;
    use hyper::service::HttpService;
    use serde_json::json;
    use spacegate_kernel::{
        layers::http_route::match_request::{SgHttpPathMatch, SgHttpRouteMatch},
        service::get_echo_service,
    };
    use testcontainers_modules::redis::REDIS_PORT;
    use tower_layer::Layer;

    use crate::PluginConfig;

    use super::*;
    #[tokio::test]
    async fn test_op_res_count_limit() {
        const GW_NAME: &str = "DEFAULT";
        const AK: &str = "3count";
        std::env::set_var("RUST_LOG", "trace");

        let docker = testcontainers::clients::Cli::default();
        let redis_container = docker.run(testcontainers_modules::redis::Redis);
        let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

        let url = format!("redis://127.0.0.1:{host_port}");
        let config = RedisCountPlugin::create(PluginConfig {
            code: Default::default(),
            name: Some("test".into()),
            spec: json! {
                {
                    "header": AUTHORIZATION.as_str(),
                }
            },
        })
        .expect("invalid config");
        global_repo().add(GW_NAME, url.as_str());
        let client = global_repo().get(GW_NAME).expect("missing client");
        let mut conn = client.get_conn().await;
        let _: () = conn.set(format!("sg:plugin:redis-count:test:*:op-res:{AK}"), 3).await.expect("fail to set");
        let layer = config.make().expect("fail to make layer");
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
                    .header(AUTHORIZATION, ak)
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
        }
    }
}
