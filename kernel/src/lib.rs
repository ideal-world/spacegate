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

use config::{http_route_dto::SgHttpRoute, ConfigListener, StaticConfig};
pub use http;
pub use hyper;
pub use spacegate_plugin;
pub use spacegate_tower::{self, helper_layers, BoxError, SgBody, SgBoxLayer, SgRequestExt, SgResponseExt};
use tardis::{
    basic::result::TardisResult,
    log::{self as tracing, info},
    tokio::{signal, task::JoinHandle},
};
mod cache_client;
pub mod config;
pub mod constants;
pub mod helpers;
pub mod server;

#[cfg(feature = "local")]
pub async fn startup_file(conf_path: impl AsRef<std::path::Path>) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    let config_listener = config::config_by_local::FileConfigListener::new(&conf_path).await?;
    Ok(config::init_with_config_listener(config_listener, ctrl_c_cancel_token()))
}
#[cfg(feature = "k8s")]
pub async fn startup_k8s(namespace: Option<&str>) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    let config_listener = config::config_by_k8s::K8sConfigListener::new(namespace).await?;
    Ok(config::init_with_config_listener(config_listener, ctrl_c_cancel_token()))
}
#[cfg(feature = "cache")]
pub async fn startup_cache(url: impl AsRef<str>, poll_interval_sec: u64) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    let config_listener = config::config_by_redis::RedisConfigListener::new(url.as_ref(), poll_interval_sec).await?;
    Ok(config::init_with_config_listener(config_listener, ctrl_c_cancel_token()))
}

pub async fn startup_static(config: StaticConfig) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    Ok(config::init_with_config_listener(config, ctrl_c_cancel_token()))
}

pub async fn startup<L>(config: L) -> Result<JoinHandle<Result<(), BoxError>>, BoxError>
where
    L: ConfigListener,
{
    Ok(config::init_with_config_listener(config, ctrl_c_cancel_token()))
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

pub fn ctrl_c_cancel_token() -> tokio_util::sync::CancellationToken {
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        let cancel_token = cancel_token.clone();
        tardis::tokio::spawn(async move {
            let _ = tardis::tokio::signal::ctrl_c().await;
            info!("Received ctrl+c signal, shutting down...");
            cancel_token.cancel();
        });
    }
    cancel_token
}
