use std::{env, time::Duration, vec};

use serde_json::{json, Value};
use spacegate_kernel::config::{
    gateway_dto::{SgGateway, SgListener, SgProtocol},
    http_route_dto::{SgHttpBackendRef, SgHttpRoute, SgHttpRouteRule},
};
use tardis::{
    basic::result::TardisResult,
    tokio::{self, time::sleep},
};

#[tokio::test]
async fn test_compression() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
    tracing_subscriber::fmt::init();
    spacegate_kernel::do_startup(
        SgGateway {
            name: "test_gw".to_string(),
            listeners: vec![SgListener { port: 8888, ..Default::default() }],
            ..Default::default()
        },
        vec![SgHttpRoute {
            gateway_name: "test_gw".to_string(),
            rules: Some(vec![SgHttpRouteRule {
                backends: Some(vec![SgHttpBackendRef {
                    name_or_host: "postman-echo.com".to_string(),
                    port: 443,
                    protocol: Some(SgProtocol::Https),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }],
    )
    .await?;
    sleep(Duration::from_millis(500)).await;
    let client = reqwest::Client::builder().gzip(true).danger_accept_invalid_certs(true).build().unwrap();
    let resp = client
        .post("http://localhost:8888/post?dd")
        .json(&json!({
            "name":"星航",
            "age":6
        }))
        .send()
        .await?;
    let resp = resp.json::<Value>().await?;
    assert!(resp.get("data").unwrap().to_string().contains("星航"));
    Ok(())
}
