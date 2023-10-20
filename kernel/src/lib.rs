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
use functions::{http_route, server};
pub use http;
pub use hyper;
use kernel_common::inner_model::{gateway::SgGateway, http_route::SgHttpRoute};
use plugins::filters::{self, SgPluginFilterDef};
use tardis::{basic::result::TardisResult, log, tokio::signal};

pub mod config;
pub mod constants;
pub mod functions;
pub mod helpers;
pub mod instance;
pub mod plugins;

pub async fn startup_k8s(namespace: Option<String>) -> TardisResult<()> {
    startup(true, namespace, None).await
}

pub async fn startup_native(conf_uri: String, check_interval_sec: u64) -> TardisResult<()> {
    startup(false, Some(conf_uri), Some(check_interval_sec)).await
}

pub async fn startup_simplify(conf_path: String, check_interval_sec: u64) -> TardisResult<()> {
    startup(false, Some(conf_path), Some(check_interval_sec)).await
}

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

#[inline]
pub fn register_filter_def(filter_def: impl SgPluginFilterDef + 'static) {
    register_filter_def_boxed(Box::new(filter_def))
}

#[inline]
pub fn register_filter_def_boxed(filter_def: Box<dyn SgPluginFilterDef>) {
    filters::register_filter_def(filter_def.get_code().to_string(), filter_def)
}
