use std::{env, time::Duration, vec};


use hyper::header::AUTHORIZATION;
use hyper::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spacegate_kernel::config::gateway_dto::SgProtocol::Https;
// use spacegate_kernel::plugins::context::SgRoutePluginContext;
// use spacegate_kernel::plugins::filters::SgPluginFilterInitDto;
use spacegate_kernel::config::{
    gateway_dto::{SgGateway, SgListener},
    http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
    plugin_filter_dto::SgRouteFilter,
};

use spacegate_plugin::{def_filter_plugin, SgPluginRepository};
use spacegate_tower::helper_layers::filter::Filter;
use spacegate_tower::{SgResponseExt};
use spacegate_tower::{BoxError};

use tardis::config::config_dto::WebClientModuleConfig;
use tardis::{
    tokio::{self, time::sleep},
    web::web_client::TardisWebClient,
};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterAuth {}

impl Filter for SgFilterAuth {
    fn filter(&self, req: hyper::Request<spacegate_tower::SgBody>) -> Result<hyper::Request<spacegate_tower::SgBody>, hyper::Response<spacegate_tower::SgBody>> {
        if req.headers().contains_key(AUTHORIZATION) {
            Ok(req)
        } else {
            Err(Response::with_code_message(StatusCode::UNAUTHORIZED, "missing authorization header"))
        }
    }
}

def_filter_plugin!("auth", SgFilterAuthPlugin, SgFilterAuth);

#[tokio::test]
async fn test_custom_plugin() -> Result<(), BoxError> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace,spacegate_plugin=trace,spacegate_tower");
    tracing_subscriber::fmt::init();
    SgPluginRepository::global().register::<SgFilterAuthPlugin>();
    spacegate_kernel::do_startup(
        SgGateway {
            name: "test_gw".to_string(),
            listeners: vec![SgListener { port: 8888, ..Default::default() }],
            ..Default::default()
        },
        vec![SgHttpRoute {
            gateway_name: "test_gw".to_string(),
            filters: Some(vec![SgRouteFilter {
                code: "auth".to_string(),
                spec: json!({}),
                ..Default::default()
            }]),
            rules: Some(vec![SgHttpRouteRule {
                backends: Some(vec![SgBackendRef {
                    name_or_host: "postman-echo.com".to_string(),
                    protocol: Some(Https),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }],
    )
    .await?;
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
    Ok(())
}
