use std::sync::Arc;

use hyper::{Request, Response};
pub mod server;
pub mod sliding_window;
pub mod status_plugin;
use serde::{Deserialize, Serialize};
use spacegate_tower::{
    extension::BackendHost,
    helper_layers::{self},
    layers::gateway::builder::SgGatewayLayerBuilder,
    SgBody, SgBoxLayer,
};
use tardis::{
    chrono::{Duration, Utc},
    tokio::{self},
};
use tower::BoxError;

use crate::MakeSgLayer;

use self::{
    sliding_window::SlidingWindowCounter,
    status_plugin::{get_status, update_status},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SgFilterStatusConfig {
    #[serde(alias = "serv_addr")]
    pub host: String,
    pub port: u16,
    pub title: String,
    /// Unhealthy threshold , if server error more than this, server will be tag as unhealthy
    pub unhealthy_threshold: u16,
    /// second
    pub interval: u64,
    pub status_cache_key: String,
    pub window_cache_key: String,
}

impl Default for SgFilterStatusConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8110,
            title: "System Status".to_string(),
            unhealthy_threshold: 3,
            interval: 5,
            status_cache_key: "spacegate:cache:plugin:status".to_string(),
            window_cache_key: sliding_window::DEFAULT_CONF_WINDOW_KEY.to_string(),
        }
    }
}

#[cfg(not(feature = "cache"))]
#[derive(Debug, Clone)]
pub struct DefaultPolicy {
    counter: Arc<RwLock<SlidingWindowCounter>>,
    unhealthy_threshold: u16,
}

#[cfg(not(feature = "cache"))]
impl spacegate_tower::helper_layers::stat::Policy for DefaultPolicy {
    fn on_request(&self, _req: &Request<SgBody>) {
        // do nothing
    }

    fn on_response(&self, resp: &Response<SgBody>) {
        if let Some(backend_host) = resp.extensions().get::<BackendHost>() {
            let backend_host = backend_host.0.clone();
            let unhealthy_threshold = self.unhealthy_threshold;
            let counter = self.counter.clone();
            if resp.status().is_server_error() {
                let now = Utc::now();
                tardis::tokio::spawn(async move {
                    let mut counter = counter.write().await;
                    let count = counter.add_and_count(now);
                    if count >= unhealthy_threshold as u64 {
                        update_status(&backend_host, status_plugin::Status::Major).await?;
                    } else {
                        update_status(&backend_host, status_plugin::Status::Minor).await?;
                    }
                    Result::<_, BoxError>::Ok(())
                });
            } else {
                tardis::tokio::spawn(async move {
                    if let Some(status) = get_status(&backend_host).await? {
                        if status != status_plugin::Status::Good {
                            update_status(&backend_host, status_plugin::Status::Good).await?;
                        }
                    }
                    Result::<_, BoxError>::Ok(())
                });
            }
        }
    }
}

#[derive(Debug, Clone)]
#[cfg(feature = "cache")]
pub struct CachePolicy {
    unhealthy_threshold: u16,
    pub interval: u64,
    status_cache_key: Arc<str>,
    window_cache_key: Arc<str>,
    gateway_name: Arc<str>,
}

impl CachePolicy {
    pub fn get_cache_key(&self, gateway_name: &str) -> String {
        format!("{}:{}", self.status_cache_key, gateway_name)
    }
}

#[cfg(feature = "cache")]
impl spacegate_tower::helper_layers::stat::Policy for CachePolicy {
    fn on_request(&self, _req: &Request<SgBody>) {
        // do nothing
    }

    fn on_response(&self, resp: &Response<SgBody>) {
        if let Some(backend_host) = resp.extensions().get::<BackendHost>() {
            let backend_host = backend_host.0.clone();
            let unhealthy_threshold = self.unhealthy_threshold;
            let cache_key = Arc::<str>::from(self.get_cache_key(&self.gateway_name));
            let gateway_name = self.gateway_name.clone();
            let interval = self.interval;
            let cache_window_key = self.window_cache_key.clone();
            if resp.status().is_server_error() {
                let now = Utc::now();

                tardis::tokio::spawn(async move {
                    let client = crate::cache::Cache::get(&gateway_name).await?;
                    let count = SlidingWindowCounter::new(Duration::seconds(interval as i64), &cache_window_key).add_and_count(now, client).await?;
                    let status = if count >= unhealthy_threshold as u64 {
                        status_plugin::Status::Major
                    } else {
                        status_plugin::Status::Minor
                    };
                    update_status(&backend_host, &cache_key, crate::cache::Cache::get(&gateway_name).await?, status).await?;
                    Result::<_, BoxError>::Ok(())
                });
            } else {
                tardis::tokio::spawn(async move {
                    let client = crate::cache::Cache::get(&gateway_name).await?;
                    if let Some(status) = get_status(&backend_host, &cache_key, &client).await? {
                        if status != status_plugin::Status::Good {
                            update_status(&backend_host, &cache_key, client, status_plugin::Status::Good).await?;
                        }
                    }
                    Result::<_, BoxError>::Ok(())
                });
            }
        }
    }
}

impl MakeSgLayer for SgFilterStatusConfig {
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError> {
        Err(BoxError::from("status plugin is only supported on gateway layer"))
    }
    fn install_on_gateway(&self, gateway: SgGatewayLayerBuilder) -> Result<SgGatewayLayerBuilder, BoxError> {
        let gateway_name = gateway.gateway_name.clone();
        let cancel_guard = gateway.cancel_token.clone();
        let config = self.clone();
        tokio::spawn(async move {
            if let Err(e) = server::launch_status_server(&config, gateway_name, cancel_guard).await {
                tracing::error!("[SG.Filter.Status] launch status server error: {e}");
            }
        });

        let gateway_name = gateway.gateway_name.clone();
        #[cfg(feature = "cache")]
        let layer = {
            let policy = CachePolicy {
                unhealthy_threshold: self.unhealthy_threshold,
                interval: self.interval,
                status_cache_key: self.status_cache_key.clone().into(),
                window_cache_key: self.window_cache_key.clone().into(),
                gateway_name,
            };
            SgBoxLayer::new(helper_layers::stat::StatLayer::new(policy))
        };
        #[cfg(not(feature = "cache"))]
        let layer = {
            let counter = Arc::new(RwLock::new(SlidingWindowCounter::new(Duration::seconds(self.interval as i64), 60)));
            let policy = DefaultPolicy {
                counter,
                unhealthy_threshold: self.unhealthy_threshold,
            };
            SgBoxLayer::new(helper_layers::stat::StatLayer::new(policy))
        };
        Ok(gateway.http_plugin(layer))
    }
}
