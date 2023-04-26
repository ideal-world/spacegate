use config::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use functions::{http_route, server};
use tardis::basic::result::TardisResult;

pub mod config;
mod functions;
pub mod plugins;

pub async fn startup(k8s_mode: bool, ext_conf_url: Option<String>, check_interval_sec: Option<u64>) -> TardisResult<()> {
    // Initialize configuration according to different modes
    let configs = config::init(k8s_mode, ext_conf_url, check_interval_sec).await?;
    for (gateway, http_routes) in configs {
        do_startup(gateway, http_routes).await?;
    }
    Ok(())
}

pub async fn do_startup(gateway: SgGateway, http_routes: Vec<SgHttpRoute>) -> TardisResult<()> {
    // Initialize service instances
    let server_insts = server::init(&gateway).await?;
    #[cfg(feature = "cache")]
    {
        // Initialize cache instances
        if let Some(url) = &gateway.parameters.redis_url {
            functions::cache::init(&gateway.name, url).await?;
        }
    }
    // Initialize route instances
    http_route::init(gateway, http_routes).await?;
    // Start service instances
    server::startup(server_insts).await
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    // Remove route instances
    http_route::remove(gateway_name).await?;
    #[cfg(feature = "cache")]
    {
        // Remove cache instances
        functions::cache::remove(gateway_name).await?;
    }
    // Shutdown service instances
    server::shutdown(gateway_name).await
}

#[cfg(test)]
mod tests {
    use std::{env, time::Duration, vec};

    use serde_json::Value;
    use tardis::{
        basic::result::TardisResult,
        tokio::{self, time::sleep},
        web::web_client::TardisWebClient,
    };

    use crate::{
        config::{
            gateway_dto::{SgGateway, SgListener, SgProtocol},
            http_route_dto::{SgHttpBackendRef, SgHttpRoute, SgHttpRouteRule},
        },
        do_startup,
    };

    #[tokio::test]
    async fn test_startup_simple() -> TardisResult<()> {
        env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
        tracing_subscriber::fmt::init();
        do_startup(
            SgGateway {
                name: "test_gw".to_string(),
                listeners: vec![SgListener { port: 8888, ..Default::default() }],
                ..Default::default()
            },
            vec![SgHttpRoute {
                gateway_name: "test_gw".to_string(),
                rules: Some(vec![SgHttpRouteRule {
                    backends: Some(vec![SgHttpBackendRef {
                        name_or_host: "anything".to_string(),
                        namespace: Some("httpbin.org".to_string()),
                        port: 80,
                        protocol: Some(SgProtocol::Http),
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
        let resp = client.get::<Value>("http://root:sss@localhost:8888/hi?dd", None).await?;
        let resp = resp.body.unwrap();
        assert!(resp.get("url").unwrap().as_str().unwrap().contains("http://localhost/anything"));
        Ok(())
    }
}
