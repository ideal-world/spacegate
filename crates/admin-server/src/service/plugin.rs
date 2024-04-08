use crate::{
    error::InternalError,
    state::{self, AppState},
};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;
use spacegate_config::{service::Discovery, PluginAttributes};

async fn get_schema_by_code<B: Discovery>(Path(plugin_code): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<Value>>, InternalError> {
    if let Some(remote) = backend.api_url().await.map_err(InternalError)? {
        reqwest::Client::new()
            .get(format!("http://{}/plugin-schema?code={plugin_code}", remote))
            .send()
            .await
            .map_err(InternalError::boxed)?
            .json::<Option<Value>>()
            .await
            .map(Json)
            .map_err(InternalError::boxed)
    } else {
        Ok(Json(None))
    }
}
async fn get_list<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<PluginAttributes>>, InternalError> {
    if let Some(remote) = backend.api_url().await.map_err(InternalError)? {
        reqwest::Client::new()
            .get(format!("http://{}/plugin-list", remote))
            .send()
            .await
            .map_err(InternalError::boxed)?
            .json::<Vec<PluginAttributes>>()
            .await
            .map(Json)
            .map_err(InternalError::boxed)
    } else {
        Ok(Json(Vec::new()))
    }
}

pub fn router<B>() -> axum::Router<state::AppState<B>>
where
    B: Discovery + Send + Sync + 'static,
{
    Router::new().route("/schema/:plugin_code", get(get_schema_by_code::<B>)).route("/list", get(get_list::<B>))
}
