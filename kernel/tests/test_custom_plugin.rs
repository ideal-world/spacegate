use std::{env, time::Duration, vec};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spacegate_kernel::{
    config::{
        gateway_dto::{SgGateway, SgListener},
        http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
        plugin_filter_dto::SgRouteFilter,
    },
    functions::http_route::SgHttpRouteMatchInst,
    plugins::filters::{SgPluginFilter, SgPluginFilterDef, SgPluginFilterKind, SgRouteFilterContext},
};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    tokio::{self, time::sleep},
    web::web_client::TardisWebClient,
    TardisFuns,
};

pub struct SgFilerAuthDef;

impl SgPluginFilterDef for SgFilerAuthDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<Box<dyn SgPluginFilter>> {
        let filter = TardisFuns::json.json_to_obj::<SgFilerAuth>(spec)?;
        Ok(Box::new(filter))
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilerAuth {}

#[async_trait]
impl SgPluginFilter for SgFilerAuth {
    fn kind(&self) -> SgPluginFilterKind {
        SgPluginFilterKind::Http
    }

    async fn init(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        if ctx.get_req_headers().contains_key("Authorization") {
            return Ok((true, ctx));
        }
        Err(TardisError::unauthorized("unauthorized", ""))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }
}

#[tokio::test]
async fn test_custom_plugin() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
    tracing_subscriber::fmt::init();
    spacegate_kernel::register_filter_def("auth", Box::new(SgFilerAuthDef));
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
    assert!(resp.body.unwrap().get("url").unwrap().as_str().unwrap().contains("http://localhost/get?dd"));
    Ok(())
}
