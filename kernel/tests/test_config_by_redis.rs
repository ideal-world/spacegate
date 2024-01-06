use std::{env, time::Duration};

mod init_cache_container;
use serde_json::Value;
use spacegate_tower::BoxError;
use tardis::{
    basic::result::TardisResult,
    cache::cache_client::TardisCacheClient,
    config::config_dto::{CacheModuleConfig, WebClientModuleConfig},
    testcontainers,
    tokio::{
        self,
        time::{sleep, timeout},
    },
    web::web_client::TardisWebClient,
};

#[tokio::test]
async fn test_config_by_redis() -> Result<(), BoxError> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace,spacegate_tower=trace");
    tracing_subscriber::fmt::init();
    let http_client = TardisWebClient::init(&WebClientModuleConfig {
        connect_timeout_sec: 1,
        request_timeout_sec: 1,
    })?;
    let docker = testcontainers::clients::Cli::default();
    let (cache_url, _x) = init_cache_container::init(&docker).await?;
    // Without keys
    assert!(spacegate_kernel::startup(false, Some(cache_url.clone()), None).await.is_err());

    let cache_client = TardisCacheClient::init(&CacheModuleConfig {
        url: cache_url.parse().expect("invalid url"),
    })
    .await?;
    cache_client
        .hset(
            "sg:conf:gateway",
            "test_gw",
            &format!(
                r#"{{
            "name":"test_gw",
            "listeners":[{{"port":8888,"protocol":"http"}}],
            "parameters":{{"redis_url":"{cache_url}"}}
        }}"#
            ),
        )
        .await?;
    cache_client
        .lpush(
            "sg:conf:route:http:test_gw",
            r#"{
            "gateway_name":"test_gw",
            "rules":[{
                "backends":[{
                    "name_or_host":"postman-echo.com",
                    "protocol":"https",
                    "port":443
                }]
            }]
        }"#,
        )
        .await?;

    // With cache url
    spacegate_kernel::startup(false, Some(cache_url.clone()), Some(1)).await?;
    sleep(Duration::from_millis(500)).await;

    let resp = http_client.get::<Value>("http://localhost:8888/get?dd1", None).await?;
    let resp = resp.body.unwrap();
    println!("resp: {:?}", resp);
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd1"));

    // Modify gateway
    cache_client
        .hset(
            "sg:conf:gateway",
            "test_gw",
            &format!(
                r#"{{
            "name":"test_gw",
            "listeners":[{{"port":8889,"protocol":"http"}}],
            "parameters":{{"redis_url":"{cache_url}"}}
        }}"#
            ),
        )
        .await?;
    cache_client.set_ex("sg:conf:change:trigger:111##gateway##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:111##gateway##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:111##gateway##test_gw", "", 1).await?;

    sleep(Duration::from_millis(1500)).await;
    let resp = http_client.get::<Value>("http://localhost:8889/get?dd2", None).await?;
    let resp = resp.body.unwrap();
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd2"));

    // Modify route
    cache_client
        .lpush(
            "sg:conf:route:http:test_gw",
            r#"{
            "gateway_name":"test_gw",
            "rules":[{
                "backends":[{
                    "name_or_host":"postman-echo.com",
                    "protocol":"https",
                    "port":443
                }]
            }]
        }"#,
        )
        .await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;

    sleep(Duration::from_millis(1500)).await;
    let resp = http_client.get::<Value>("http://localhost:8889/get?dd3", None).await?;
    let resp = resp.body.unwrap();
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd3"));

    // Remove gateway
    cache_client.hdel("sg:conf:gateway", "test_gw").await?;
    cache_client.set_ex("sg:conf:change:trigger:333##gateway##test_gw", "", 1).await?;

    sleep(Duration::from_millis(1500)).await;
    let resp = timeout(Duration::from_secs(1), http_client.get_to_str("http://localhost:8889/get?dd4", None)).await;
    assert!(resp.is_err());

    Ok(())
}
