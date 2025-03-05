use crate::{
    error::InternalError,
    service::discovery::InstanceApi,
    state::{self, AppState},
};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;
use spacegate_config::service::Instance;
use spacegate_config::{service::Discovery, BoxError, PluginAttributes};
use std::{collections::HashMap, sync::OnceLock, time::Duration};
use tokio::{sync::RwLock, time::Instant};
use tracing::info;

static ATTR_CACHE: OnceLock<RwLock<HashMap<String, PluginAttributes>>> = OnceLock::new();

async fn sync_attr_cache<B: Discovery>(backend: &B, refresh: bool) -> Result<(), BoxError> {
    static NEXT_SYNC: OnceLock<RwLock<Instant>> = OnceLock::new();
    const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);
    let next_sync = NEXT_SYNC.get_or_init(|| RwLock::new(Instant::now())).read().await;
    if next_sync.elapsed() != Duration::ZERO || refresh {
        drop(next_sync);
        let mut cache = ATTR_CACHE.get_or_init(Default::default).write().await;
        if let Some(remote) = backend.instances().await?.into_iter().next() {
            info!("refresh plugin attr from: {}", remote.id());
            let attrs = InstanceApi::new(&remote).plugin_list().await?;
            cache.clear();
            cache.extend(attrs.into_iter().map(|attr| (attr.code.to_string(), attr)));
        };
        let mut next_sync = NEXT_SYNC.get_or_init(|| RwLock::new(Instant::now())).write().await;
        *next_sync = Instant::now() + SYNC_INTERVAL;
    }
    Ok(())
}

async fn get_schema_by_code<B: Discovery>(Path(plugin_code): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<Value>>, InternalError> {
    if let Some(remote) = backend.instances().await.map_err(InternalError)?.into_iter().next() {
        InstanceApi::new(&remote).schema(&plugin_code).await.map(Json).map_err(InternalError)
    } else {
        Ok(Json(None))
    }
}
async fn get_list<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<String>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(Default::default).read().await.keys().cloned().collect()))
}
async fn get_attr_all<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<PluginAttributes>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(Default::default).read().await.values().cloned().collect()))
}
async fn get_attr<B: Discovery>(Path(plugin_code): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<PluginAttributes>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(Default::default).read().await.get(&plugin_code).cloned()))
}

pub fn router<B>() -> axum::Router<state::AppState<B>>
where
    B: Discovery + Send + Sync + 'static,
{
    Router::new()
        .route("/schema/:plugin_code", get(get_schema_by_code::<B>))
        .route("/list", get(get_list::<B>))
        .route("/attr-all", get(get_attr_all::<B>))
        .route("/attr/:plugin_code", get(get_attr::<B>))
}
