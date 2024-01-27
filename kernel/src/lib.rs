//! **A library-first, lightweight, high-performance, cloud-native supported API gateway🪐**
//!
//! ## 🚀 Installation
//!
//! see [installation.md](https://github.com/ideal-world/spacegate/blob/master/docs/k8s/installation.md)
//!
//! ## Special instructions for configuration
//! ### Setting HTTP Route Priority
//! You can specify the priority of an httproute by adding a priority field in the annotations section of the route.
//! A higher value for the priority field indicates a higher priority. The httproute library stores the priority
//! value using the i64 data type, so the maximum and minimum values for the priority are [i64::MAX]
//! (https://doc.rust-lang.org/std/primitive.i64.html#associatedconstant.MAX) and
//! [i64::MIN](https://doc.rust-lang.org/std/primitive.i64.html#associatedconstant.MIN) respectively.
//!
//! If the priority field is not present in an httproute, its priority will be default to 0, and the default priority
//! will be determined based on the creation order (earlier routes will have higher priority).
//!
//! Note: Trace-level logs will print the contents of both the request and response bodies,
//! potentially causing significant performance overhead. It is recommended to use debug level
//! logs at most.

#![warn(clippy::unwrap_used)]
use config::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
pub use http;
pub use hyper;
pub use spacegate_plugin;
pub use spacegate_tower::{self, helper_layers, BoxError, SgBody, SgBoxLayer, SgRequestExt, SgResponseExt};
use tardis::{
    basic::result::TardisResult,
    log::{self as tracing, instrument},
    tokio::signal,
};
mod cache_client;
pub mod config;
pub mod constants;
pub mod helpers;
pub mod server;
// pub mod instance;
// pub mod plugins;

#[inline]
pub async fn startup_k8s(namespace: Option<String>) -> Result<(), BoxError> {
    startup(true, namespace, None).await
}

#[inline]
pub async fn startup_native(conf_uri: String, check_interval_sec: u64) -> Result<(), BoxError> {
    startup(false, Some(conf_uri), Some(check_interval_sec)).await
}

#[inline]
pub async fn startup_simplify(conf_path: String, check_interval_sec: u64) -> Result<(), BoxError> {
    startup(false, Some(conf_path), Some(check_interval_sec)).await
}

pub async fn startup(k8s_mode: bool, namespace_or_conf_uri: Option<String>, check_interval_sec: Option<u64>) -> Result<(), BoxError> {
    // Initialize configuration according to different modes
    let configs = config::init(k8s_mode, namespace_or_conf_uri, check_interval_sec).await?;
    for (gateway, http_routes) in configs {
        do_startup(gateway, http_routes).await?;
    }
    Ok(())
}

#[instrument(skip(gateway))]
pub async fn do_startup(gateway: SgGateway, http_routes: Vec<SgHttpRoute>) -> Result<(), BoxError> {
    let gateway_name = gateway.name.clone();
    #[cfg(feature = "cache")]
    {
        // Initialize cache instances
        if let Some(url) = &gateway.parameters.redis_url {
            tracing::trace!("Initialize cache client...url:{url}");
            cache_client::init(gateway_name.clone(), url).await?;
        }
    }
    // Initialize service instances
    let running_gateway = server::RunningSgGateway::create(gateway, http_routes)?;
    server::RunningSgGateway::global_save(gateway_name, running_gateway);
    Ok(())
}

#[instrument]
pub async fn update_route(gateway_name: &str, http_routes: Vec<SgHttpRoute>) -> Result<(), BoxError> {
    server::RunningSgGateway::global_update(gateway_name, http_routes).await
}

#[instrument]
pub async fn shutdown(gateway_name: &str) -> Result<(), BoxError> {
    // Remove route instances
    // http_route::remove(gateway_name).await?;
    #[cfg(feature = "cache")]
    {
        // Remove cache instances
        cache_client::remove(gateway_name).await?;
    }
    // Shutdown service instances
    if let Some(gateway) = server::RunningSgGateway::global_remove(gateway_name) {
        gateway.shutdown().await;
    }
    Ok(())
}

pub async fn wait_graceful_shutdown() -> TardisResult<()> {
    match signal::ctrl_c().await {
        Ok(_) => {
            let instances = server::RunningSgGateway::global_store().lock().expect("fail to lock").drain().collect::<Vec<_>>();
            for (_, inst) in instances {
                inst.shutdown().await;
            }
            tracing::info!("Received ctrl+c signal, shutting down...");
        }
        Err(error) => {
            tracing::error!("Received the ctrl+c signal, but with an error: {error}");
        }
    }
    Ok(())
}
