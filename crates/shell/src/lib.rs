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
//!
//!
//! ## startup
//! ### static config
//! see [`startup_static`]
//! ### by config file
//! see [`startup_file`]
//! ### by k8s resource
//! see [`startup_k8s`]
//! ### by redis
//! see [`startup_redis`]
//!
#![warn(clippy::unwrap_used)]

use config::SgHttpRoute;
pub use hyper;
pub use spacegate_config::model;
pub use spacegate_config::model::{BoxError, BoxResult};
use spacegate_config::service::{CreateListener, Retrieve};
use spacegate_config::Config;
pub use spacegate_kernel as kernel;
pub use spacegate_plugin as plugin;
use tokio::signal;
use tracing::{info, instrument};

pub mod config;
pub mod constants;
pub mod extension;
pub mod server;

#[cfg(feature = "ext-redis")]
pub use spacegate_ext_redis;

#[cfg(feature = "fs")]
/// # Startup the gateway by config file
/// The `conf_dir` is the path of the configuration dir.
pub async fn startup_file(conf_dir: impl AsRef<std::path::Path>) -> Result<(), BoxError> {
    use spacegate_config::service::{config_format::Json, fs::Fs};
    let config = Fs::new(conf_dir, Json::default());
    startup(config).await
}
#[cfg(feature = "k8s")]
/// # Startup the gateway by k8s resource
/// The `namespace` is the k8s namespace.
/// If the `namespace` is None, it will use the default namespace.
pub async fn startup_k8s(namespace: Option<&str>) -> Result<(), BoxError> {
    // use spacegate_config::service::backend::k8s::K8s;
    // let namespace = namespace.unwrap_or("default");
    // let config = K8s::new(namespace, kube::Client::try_default().await?);
    // startup(config)
    unimplemented!()
}
#[cfg(feature = "cache")]
/// # Startup the gateway by redis
/// The `url` is the redis url, and the json format will be used.
pub async fn startup_redis(url: impl Into<String>) -> Result<(), BoxError> {
    use spacegate_config::service::{config_format::Json, redis::Redis};
    let config = Redis::new(url.into(), Json::default())?;
    startup(config).await
}

/// # Startup the gateway by static config
/// The `config` is the static config.
pub async fn startup_static(config: Config) -> Result<(), BoxError> {
    use spacegate_config::service::memory::Memory;
    let config = Memory::new(config);
    startup(config).await
}

/// # Startup the gateway
/// The `config` could be any type that implements [`spacegate_config::service::CreateListener`] and [`spacegate_config::service::Retrieve`] trait.
///
/// ## Errors
/// If the config is invalid, it will return a BoxError.
#[instrument(fields(listener = (L::CONFIG_LISTENER_NAME)), skip(config))]
pub async fn startup<L>(config: L) -> Result<(), BoxError>
where
    L: CreateListener + Retrieve + 'static,
{
    info!("Spacegate Meta Info: {:?}", Meta::new());
    info!("Starting gateway...");
    config::startup_with_shutdown_signal(config, ctrl_c_cancel_token()).await
}

#[derive(Debug, Clone, Copy)]
pub struct Meta {
    pub version: &'static str,
    // pub commit: &'static str,
}

impl Meta {
    const DEFAULT: Meta = Self {
        version: env!("CARGO_PKG_VERSION"),
        // commit: tardis::utils::build_info::git_version!(cargo_prefix = "cargo:", fallback = "unknown"),
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

pub async fn wait_graceful_shutdown() {
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
}

pub fn ctrl_c_cancel_token() -> tokio_util::sync::CancellationToken {
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        let cancel_token = cancel_token.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            info!("Received ctrl+c signal, shutting down...");
            cancel_token.cancel();
        });
    }
    cancel_token
}
