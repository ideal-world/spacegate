use std::{env, time::Duration, vec};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spacegate_kernel::config::gateway_dto::SgProtocol::Https;
use spacegate_kernel::plugins::context::SgRoutePluginContext;
use spacegate_kernel::plugins::filters::SgPluginFilterInitDto;
use spacegate_kernel::{
    config::{
        gateway_dto::{SgGateway, SgListener},
        http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
        plugin_filter_dto::SgRouteFilter,
    },
    plugins::filters::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef},
};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    tokio::{self, time::sleep},
    web::web_client::TardisWebClient,
    TardisFuns,
};

pub struct SgFilterAuthDef;

impl SgPluginFilterDef for SgFilterAuthDef {
    fn get_code(&self) -> &'static str {
        "auth"
    }
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterAuth>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterAuth {}

#[async_trait]
impl SgPluginFilter for SgFilterAuth {
    async fn init(&mut self, _: &SgPluginFilterInitDto) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if ctx.request.get_headers().contains_key("Authorization") {
            return Ok((true, ctx));
        }
        Err(TardisError::unauthorized("unauthorized", ""))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}

#[tokio::test]
async fn test_custom_plugin() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
    tracing_subscriber::fmt::init();
    spacegate_kernel::register_filter_def(SgFilterAuthDef);
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
    let client = TardisWebClient::init(100)?;
    let resp = client.get_to_str("http://localhost:8888/get?dd", None).await?;
    assert_eq!(resp.code, 401);

    let resp = client.get::<Value>("http://localhost:8888/get?dd", Some(vec![("Authorization".to_string(), "xxxxx".to_string())])).await?;
    assert_eq!(resp.code, 200);
    assert!(resp.body.unwrap().get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd"));
    Ok(())
}
