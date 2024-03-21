use std::sync::Arc;

use hyper::{body::Bytes, header::HeaderName, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};

use spacegate_ext_redis::{
    global_repo,
    redis::{AsyncCommands, Script},
    RedisClient, RedisClientRepoError,
};
use spacegate_kernel::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::{
        check::{redis::RedisCheck, CheckLayer},
        function::{FnLayer, FnLayerMethod, Inner},
    },
    layers::{
        gateway::builder::SgGatewayLayerBuilder,
        http_route::{
            builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
            match_request::SgHttpPathMatch,
        },
    },
    BoxError, BoxResult, SgBody, SgBoxLayer, SgResponseExt,
};

use crate::{def_plugin, MakeSgLayer, PluginError};
use spacegate_kernel::ret_error;

use super::format_key;

#[derive(Serialize, Deserialize)]
pub struct OpresFreqLimitConfig {
    pub prefix: String,
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
        let Some(key) = format_key(&req, &matched, &self.header) else {
            return PluginError::status::<RedisLimitPlugin, 401>(format!("missing header {}", self.header.as_str())).into();
        };
        let pass: bool = ret_error!(self.script.key(key).invoke_async(&mut conn).await.map_err(PluginError::internal_error::<RedisLimitPlugin>));
        if !pass {
            return PluginError::status::<RedisLimitPlugin, 429>("too many request, please try later").into();
        }
        inner.call(req).await
    }
}

impl OpresFreqLimitConfig {
    pub fn create_check(&self, gateway_name: &str) -> BoxResult<RedisCheck> {
        let check_script = Script::new(include_str!("./redis_freq_limit/check.lua"));
        let check = RedisCheck {
            check_script: Some(check_script.into()),
            response_script: None,
            key_prefix: <Arc<str>>::from(format!("{}:frequency", self.prefix)),
            client: global_repo().get(gateway_name).ok_or(RedisClientRepoError::new(gateway_name, "missing redis client"))?,
            on_fail: Some((StatusCode::TOO_MANY_REQUESTS, Bytes::from_static(b"too many request, please try later"))),
        };
        Ok(check)
    }
}

impl MakeSgLayer for OpresFreqLimitConfig {
    fn make_layer(&self) -> BoxResult<spacegate_kernel::SgBoxLayer> {
        let check_script = Script::new(include_str!("./redis_freq_limit/check.lua"));
        let method = Arc::new(RedisLimit {
            prefix: self.prefix.clone(),
            header: HeaderName::from_bytes(self.header.as_bytes()).map_err(|e| PluginError::internal_error::<RedisLimitPlugin>(e))?,
            script: check_script,
        });
        let layer = FnLayer::new(method);
        Ok(SgBoxLayer::new(layer))
    }
}

def_plugin!("opres-freq-limit", RedisLimitPlugin, OpresFreqLimitConfig);

// #[cfg(test)]
// mod test {
//     use std::time::Duration;

//     use http::Request;
//     use spacegate_shell::{
//         hyper::service::HttpService,
//         kernel::{
//             extension::MatchedSgRouter,
//             layers::http_route::match_request::{SgHttpPathMatch, SgHttpRouteMatch},
//             service::get_echo_service,
//             Layer,
//         },
//         plugin::Plugin,
//         spacegate_ext_redis::redis::AsyncCommands,
//         SgBody,
//     };
//     use tardis::{
//         basic::tracing::TardisTracing,
//         serde_json::json,
//         testcontainers,
//         tokio::{self},
//     };
//     use testcontainers_modules::redis::REDIS_PORT;

//     use super::*;
//     #[tokio::test]
//     async fn test_op_res_freq_limit() {
//         const GW_NAME: &str = "DEFAULT";
//         const AK: &str = "3qpm";
//         std::env::set_var("RUST_LOG", "trace");
//         let _ = TardisTracing::initializer().with_fmt_layer().with_env_layer().init_standalone();

//         let docker = testcontainers::clients::Cli::default();
//         let redis_container = docker.run(testcontainers_modules::redis::Redis);
//         let host_port = redis_container.get_host_port_ipv4(REDIS_PORT);

//         let url = format!("redis://127.0.0.1:{host_port}");
//         let config = OpresFreqLimitPlugin::create(json! {
//             {
//                 "prefix": "bios:limit"
//             }
//         })
//         .expect("invalid config");
//         global_repo().add(GW_NAME, url.as_str());
//         let client = global_repo().get(GW_NAME).expect("missing client");
//         let mut conn = client.get_conn().await;
//         let _: () = conn.set(format!("bios:limit:frequency:*:op-res:{AK}"), 3).await.expect("fail to set");
//         let layer = config.make_layer_with_gateway_name(GW_NAME).expect("fail to make layer");
//         let backend_service = get_echo_service();
//         let mut service = layer.layer(backend_service);
//         {
//             fn gen_req(ak: &str) -> Request<SgBody> {
//                 Request::builder()
//                     .uri("http://127.0.0.1/op-res/example")
//                     .method("GET")
//                     .extension(GatewayName::new(GW_NAME))
//                     .extension(MatchedSgRouter(
//                         SgHttpRouteMatch {
//                             path: Some(SgHttpPathMatch::Prefix("op-res".to_string())),
//                             ..Default::default()
//                         }
//                         .into(),
//                     ))
//                     .header("Bios-Authorization", format!("{ak}:sign"))
//                     .body(SgBody::empty())
//                     .expect("fail to build")
//             }
//             for _times in 0..3 {
//                 let resp = service.call(gen_req(AK)).await.expect("infallible");
//                 let (parts, body) = resp.into_parts();
//                 let body = body.dump().await.expect("fail to dump");
//                 println!("body: {body:?}, parts: {parts:?}");
//                 assert!(parts.status.is_success());
//             }
//             let resp = service.call(gen_req(AK)).await.expect("infallible");
//             let (parts, body) = resp.into_parts();
//             let body = body.dump().await.expect("fail to dump");
//             println!("body: {body:?}, parts: {parts:?}");
//             assert!(parts.status.is_client_error());
//             tokio::time::sleep(Duration::from_secs(61)).await;
//             let resp = service.call(gen_req(AK)).await.expect("infallible");
//             let (parts, body) = resp.into_parts();
//             let body = body.dump().await.expect("fail to dump");
//             println!("body: {body:?}, parts: {parts:?}");
//             assert!(parts.status.is_success());
//         }
//     }
// }
