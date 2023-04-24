use config::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use functions::{http_route, server};
use tardis::basic::result::TardisResult;

mod config;
mod functions;
mod plugins;

pub async fn startup(k8s_mode: bool, ext_conf_url: Option<String>) -> TardisResult<()> {
    // Initialize configuration according to different modes
    let configs = config::init(k8s_mode, ext_conf_url).await?;
    for (gateway, http_routes) in configs {
        do_startup(gateway, http_routes).await?;
    }
    Ok(())
}

async fn do_startup(gateway: SgGateway, http_routes: Vec<SgHttpRoute>) -> TardisResult<()> {
    // Initialize service instances
    let server_insts = server::init(&gateway).await?;
    #[cfg(feature = "cache")]
    {
        // Initialize cache instances
        if let Some(url) = &gateway.parameters.redis_url {
            crate::functions::cache::init(&gateway.name, url).await?;
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
        cache::remove(gateway_name).await?;
    }
    // Shutdown service instances
    server::shutdown(gateway_name).await
}
