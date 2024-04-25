use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::{extract::State, routing::get, Json, Router};
use serde_json::Value;
use spacegate_config::{service::Discovery, BackendHost, BoxError, PluginAttributes};
use tokio::{sync::RwLock, time::Instant};

use crate::{error::InternalError, state::AppState};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_HEALTH_CACHE_EXPIRE: Duration = Duration::from_secs(1);
static HEALTH_CACHE: OnceLock<Arc<RwLock<(bool, Instant)>>> = OnceLock::new();
fn health_cache() -> Arc<RwLock<(bool, Instant)>> {
    HEALTH_CACHE.get_or_init(|| Arc::new(RwLock::new((false, Instant::now())))).clone()
}
async fn set_health_cache(health: bool) {
    let expire = Instant::now() + DEFAULT_HEALTH_CACHE_EXPIRE;
    let cache = health_cache();
    let mut wg = cache.write().await;
    *wg = (health, expire)
}
async fn get_health_cache() -> Option<bool> {
    let cache = health_cache();
    let (health, expire) = *cache.read().await;
    if expire.elapsed() >= Duration::ZERO {
        None
    } else {
        Some(health)
    }
}

pub trait Instance: Send + Sync + 'static {
    fn api_url(&self) -> &str;
    fn timeout(&self) -> Duration;
}

impl Instance for String {
    fn api_url(&self) -> &str {
        self
    }

    fn timeout(&self) -> Duration {
        DEFAULT_TIMEOUT
    }
}

impl dyn Instance {
    /// get api url
    pub fn url(&self, path: &str) -> String {
        format!("http://{base}/{path}", base = self.api_url(), path = path.trim_start_matches('/'))
    }
    /// check server health
    pub async fn health(&self) -> bool {
        if let Some(health) = get_health_cache().await {
            health
        } else {
            use reqwest::Client;
            let client = Client::default();
            let timeout = self.timeout();
            let health = client.get(self.url("/health")).timeout(timeout).send().await.is_ok_and(|x| x.status().is_success());
            set_health_cache(health).await;
            health
        }
    }

    pub async fn schema(&self, plugin_code: &str) -> Result<Option<Value>, BoxError> {
        let resp = reqwest::Client::new()
            .get(format!("http://{base}/plugin-schema?code={plugin_code}", base = self.api_url()))
            .timeout(self.timeout())
            .send()
            .await?
            .json::<Option<Value>>()
            .await?;
        Ok(resp)
    }

    pub async fn plugin_list(&self) -> Result<Vec<PluginAttributes>, BoxError> {
        let attrs =
            reqwest::Client::new().get(format!("http://{base}/plugin-list", base = self.api_url())).timeout(self.timeout()).send().await?.json::<Vec<PluginAttributes>>().await?;
        Ok(attrs)
    }
}

async fn health<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<bool>, InternalError> {
    if let Some(remote) = backend.api_url().await.map_err(InternalError)? {
        Ok(Json(<dyn Instance>::health(&remote).await))
    } else {
        Ok(Json(false))
    }
}

async fn backends<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<BackendHost>>, InternalError> {
    backend.backends().await.map(Json).map_err(InternalError)
}

pub fn router<B>() -> axum::Router<AppState<B>>
where
    B: Discovery + Send + Sync + 'static,
{
    Router::new()
    .route("/health", get(health::<B>))
    .route("/backends", get(backends::<B>))
}
