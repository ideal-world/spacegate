use std::{sync::Arc, time::Duration};

use axum::{extract::State, routing::get, Json, Router};
use serde_json::Value;
use spacegate_config::{service::Discovery, BoxError, PluginAttributes};

use crate::{error::InternalError, state::AppState};

pub struct K8sInstance {
    pub name: Arc<str>,
    pub namespace: Arc<str>,
}

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

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
        use reqwest::Client;
        let client = Client::default();
        let timeout = self.timeout();
        client.get(self.url("/health")).timeout(timeout).send().await.is_ok_and(|x| x.status().is_success())
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

pub fn router<B>() -> axum::Router<AppState<B>>
where
    B: Discovery + Send + Sync + 'static,
{
    Router::new().route("/health", get(health::<B>))
}
