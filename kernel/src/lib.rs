#![warn(clippy::unwrap_used)]
use config::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use functions::{http_route, server};
pub use http;
pub use hyper;
use plugins::filters::{self, SgPluginFilterDef};
use tardis::{basic::result::TardisResult, log, tokio::signal};

pub mod config;
pub mod constants;
pub mod functions;
pub mod helpers;
pub mod instance;
pub mod plugins;

pub async fn startup(k8s_mode: bool, namespace_or_conf_uri: Option<String>, check_interval_sec: Option<u64>) -> TardisResult<()> {
    // Initialize configuration according to different modes
    let configs = config::init(k8s_mode, namespace_or_conf_uri, check_interval_sec).await?;
    for (gateway, http_routes) in configs {
        do_startup(gateway, http_routes).await?;
    }
    Ok(())
}

pub async fn do_startup(gateway: SgGateway, http_routes: Vec<SgHttpRoute>) -> TardisResult<()> {
    // Initialize service instances
    let server_insts = server::init(&gateway).await?;
    let gateway_name = &gateway.name.clone();
    #[cfg(feature = "cache")]
    {
        // Initialize cache instances
        if let Some(url) = &gateway.parameters.redis_url {
            log::trace!("Initialize cache client...url:{url}");
            functions::cache_client::init(gateway_name, url).await?;
        }
    }
    // Initialize route instances
    http_route::init(gateway, http_routes).await?;
    // Start service instances
    server::startup(gateway_name, server_insts).await
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    // Remove route instances
    http_route::remove(gateway_name).await?;
    #[cfg(feature = "cache")]
    {
        // Remove cache instances
        functions::cache_client::remove(gateway_name).await?;
    }
    // Shutdown service instances
    server::shutdown(gateway_name).await
}

pub async fn wait_graceful_shutdown() -> TardisResult<()> {
    match signal::ctrl_c().await {
        Ok(_) => {
            log::info!("Received ctrl+c signal, shutting down...");
        }
        Err(error) => {
            log::error!("Received the ctrl+c signal, but with an error: {error}");
        }
    }
    Ok(())
}

pub fn register_filter_def(code: &str, filter_def: Box<dyn SgPluginFilterDef>) {
    filters::register_filter_def(code, filter_def)
}
