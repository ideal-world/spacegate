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

use config::SgHttpRoute;
pub use hyper;
use spacegate_config::service::backend::memory::Memory;
use spacegate_config::service::{CreateListener, Retrieve};
use spacegate_config::Config;
pub use spacegate_kernel as kernel;
pub use spacegate_kernel::{BoxError, SgBody, SgBoxLayer, SgRequestExt, SgResponseExt};
pub use spacegate_plugin as plugin;
use tardis::log::instrument;
use tardis::{
    basic::result::TardisResult,
    log::{self as tracing, info},
    tokio::{signal, task::JoinHandle},
};

#[cfg(feature = "cache")]
pub mod cache_client;
pub mod config;
pub mod constants;
pub mod extension;
pub mod helpers;
pub mod server;

#[cfg(feature = "local")]
pub async fn startup_file(conf_path: impl AsRef<std::path::Path>) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    use spacegate_config::service::{backend::fs::Fs, config_format::Json};
    let config = Fs::new(conf_path, Json::default());
    startup(config)
}
#[cfg(feature = "k8s")]
pub async fn startup_k8s(namespace: Option<&str>) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    use spacegate_config::service::backend::k8s::K8s;
    let namespace = namespace.unwrap_or("default");
    let config = K8s::new(namespace, kube::Client::try_default().await?);
    unimplemented!("")
    // startup(config)
}
#[cfg(feature = "cache")]
pub async fn startup_cache(_url: impl AsRef<str>, _poll_interval_sec: u64) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    unimplemented!("cache feature is not implemented yet")
    // let config_listener = config::config_by_redis::RedisConfigListener::new(url.as_ref(), poll_interval_sec).await?;
    // startup(config_listener)
}

pub fn startup_static(config: Config) -> Result<JoinHandle<Result<(), BoxError>>, BoxError> {
    let config = Memory::new(config);
    startup(config)
}

#[instrument(fields(listener = (L::CONFIG_LISTENER_NAME)), skip(config))]
pub fn startup<L>(config: L) -> Result<JoinHandle<Result<(), BoxError>>, BoxError>
where
    L: CreateListener + Retrieve + 'static,
{
    info!("Starting spacegate...");
    info!("Spacegate Meta Info: {:?}", Meta::new());
    Ok(config::init_with_config(config, ctrl_c_cancel_token()))
}

#[derive(Debug, Clone, Copy)]
pub struct Meta {
    pub version: &'static str,
    pub commit: &'static str,
}

impl Meta {
    const DEFAULT: Meta = Self {
        version: env!("CARGO_PKG_VERSION"),
        commit: tardis::utils::build_info::git_version!(cargo_prefix = "cargo:", fallback = "unknown"),
    };
    pub const fn new() -> Self {
        Self::DEFAULT
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self::DEFAULT
    }
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
