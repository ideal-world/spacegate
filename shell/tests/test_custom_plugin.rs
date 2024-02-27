use std::{env, time::Duration, vec};

use hyper::header::AUTHORIZATION;
use hyper::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spacegate_config::model::SgBackendProtocol;
use spacegate_shell::config::BackendHost;
use spacegate_shell::config::{SgBackendRef, SgGateway, SgHttpRoute, SgHttpRouteRule, SgListener, SgRouteFilter};

use spacegate_kernel::helper_layers::filter::Filter;
use spacegate_kernel::SgResponseExt;
use spacegate_kernel::{BoxError, BoxResult};
use spacegate_plugin::{def_filter_plugin, SgPluginRepository};
use spacegate_shell::ctrl_c_cancel_token;

use tardis::config::config_dto::WebClientModuleConfig;
use tardis::{
    tokio::{self, time::sleep},
    web::web_client::TardisWebClient,
};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterAuth {}

impl Filter for SgFilterAuth {
    fn filter(&self, req: hyper::Request<spacegate_kernel::SgBody>) -> Result<hyper::Request<spacegate_kernel::SgBody>, hyper::Response<spacegate_kernel::SgBody>> {
        if req.headers().contains_key(AUTHORIZATION) {
            Ok(req)
        } else {
            Err(Response::with_code_message(StatusCode::UNAUTHORIZED, "missing authorization header"))
        }
    }
}

def_filter_plugin!("auth", SgFilterAuthPlugin, SgFilterAuth);

#[tokio::test]
async fn test_custom_plugin() -> BoxResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_shell=trace,spacegate_plugin=trace,spacegate_kernel");
    tracing_subscriber::fmt::init();
    SgPluginRepository::global().register::<SgFilterAuthPlugin>();
    let localset = tokio::task::LocalSet::new();
    localset.spawn_local(async move {
        let token = ctrl_c_cancel_token();
        let _server = spacegate_shell::server::RunningSgGateway::create(
            SgGateway {
                name: "test_gw".to_string(),
                listeners: vec![SgListener { port: 8888, ..Default::default() }],
                ..Default::default()
            },
            vec![SgHttpRoute {
                gateway_name: "test_gw".to_string(),
                filters: vec![SgRouteFilter {
                    code: "auth".to_string(),
                    spec: json!({}),
                    ..Default::default()
                }],
                rules: vec![SgHttpRouteRule {
                    backends: vec![SgBackendRef {
                        host: BackendHost::Host { host: "postman-echo.com".into() },
                        protocol: Some(SgBackendProtocol::Https),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            token.clone(),
        )
        .expect("fail to start up server");
        token.cancelled().await
    });
    localset
        .run_until(async move {
            sleep(Duration::from_millis(500)).await;
            let client = TardisWebClient::init(&WebClientModuleConfig {
                connect_timeout_sec: 100,
                ..Default::default()
            })?;
            let resp = client.get_to_str("http://localhost:8888/get?dd", None).await?;
            assert_eq!(resp.code, 401);

            let resp = client.get::<Value>("http://localhost:8888/get?dd", [("Authorization".to_string(), "xxxxx".to_string())]).await?;
            assert_eq!(resp.code, 200);
            assert!(resp.body.unwrap().get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd"));
            Ok::<(), BoxError>(())
        })
        .await
}
