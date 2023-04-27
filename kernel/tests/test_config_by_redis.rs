use std::{env, time::Duration};

mod init_cache_container;
use serde_json::Value;
use tardis::{
    basic::result::TardisResult,
    cache::cache_client::TardisCacheClient,
    testcontainers,
    tokio::{self, time::sleep},
    web::web_client::TardisWebClient,
};

#[tokio::test]
async fn test_config_by_redis() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
    tracing_subscriber::fmt::init();
    let http_client = TardisWebClient::init(100)?;
    let docker = testcontainers::clients::Cli::default();
    let (cache_url, _x) = init_cache_container::init(&docker).await?;

    // Without cache url
    assert!(spacegate_kernel::startup(false, None, None).await.is_err());

    // Without keys
    assert!(spacegate_kernel::startup(false, Some(cache_url.clone()), None).await.is_err());

    let cache_client = TardisCacheClient::init(&cache_url).await?;
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
            &format!(
                r#"{{
            "gateway_name":"test_gw",
            "rules":[{{
                "backends":[{{
                    "name_or_host":"postman-echo.com",
                    "port":80
                }}]
            }}]
        }}"#
            ),
        )
        .await?;

    // With cache url
    spacegate_kernel::startup(false, Some(cache_url.clone()), Some(1)).await?;
    sleep(Duration::from_millis(500)).await;

    let resp = http_client.get::<Value>("http://localhost:8888/get?dd", None).await?;
    let resp = resp.body.unwrap();
    println!("resp: {:?}", resp);
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("http://localhost/get?dd"));

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
    let resp = http_client.get::<Value>("http://localhost:8889/get?dd", None).await?;
    let resp = resp.body.unwrap();
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("http://localhost/get?dd"));

    // Modify route
    cache_client
        .lpush(
            "sg:conf:route:http:test_gw",
            &format!(
                r#"{{
            "gateway_name":"test_gw",
            "rules":[{{
                "backends":[{{
                    "name_or_host":"postman-echo.com",
                    "protocol":"https",
                    "port":443
                }}]
            }}]
        }}"#
            ),
        )
        .await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;
    cache_client.set_ex("sg:conf:change:trigger:222##httproute##test_gw", "", 1).await?;

    sleep(Duration::from_millis(1500)).await;
    let resp = http_client.get::<Value>("http://localhost:8889/get?dd", None).await?;
    let resp = resp.body.unwrap();
    assert!(resp.get("url").unwrap().as_str().unwrap().contains("https://localhost/get?dd"));

    // Remove gateway
    cache_client.hdel("sg:conf:gateway", "test_gw").await?;
    cache_client.set_ex("sg:conf:change:trigger:333##gateway##test_gw", "", 1).await?;

    sleep(Duration::from_millis(1500)).await;
    assert!(http_client.get_to_str("http://localhost:8889/get?dd", None).await.is_err());

    Ok(())
}
