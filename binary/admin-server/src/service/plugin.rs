use crate::{
    error::InternalError,
    service::discovery::InstanceApi,
    state::{self, AppState},
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use spacegate_config::service::Instance;
use spacegate_config::{
    service::{Discovery, Retrieve},
    BoxError, PluginAttributes, PluginConfig, PluginInstanceId,
};
use spacegate_plugin_wasm::{config::OciAuthConfig, error::WasmHostError, fetch::fetch_wasm_image_file_sync_with_auth};
use std::{collections::HashMap, future::Future, sync::OnceLock, time::Duration};
use tokio::{
    sync::{Mutex, RwLock},
    time::Instant,
};
use tracing::info;

const ATTR_SYNC_INTERVAL: Duration = Duration::from_secs(3600);
static ATTR_CACHE: OnceLock<PluginAttrCache> = OnceLock::new();

/// Stores plugin attributes discovered from a running Spacegate instance.
struct PluginAttrCache {
    /// Attributes indexed by plugin code for list and detail endpoints.
    attrs: RwLock<HashMap<String, PluginAttributes>>,
    /// Earliest time at which an automatic refresh may run again.
    next_sync: Mutex<Instant>,
}

impl PluginAttrCache {
    /// Creates an empty cache that is immediately eligible for refresh.
    fn new() -> Self {
        Self {
            attrs: RwLock::new(HashMap::new()),
            next_sync: Mutex::new(Instant::now()),
        }
    }

    /// Refreshes attributes from the configured Spacegate discovery backend.
    async fn sync<B: Discovery>(&self, backend: &B, force: bool) -> Result<(), BoxError> {
        self.sync_with(force, || async {
            let Some(remote) = backend.instances().await?.into_iter().next() else {
                return Err("spacegate instance not found".into());
            };
            info!(instance = remote.id(), api_url = remote.api_url(), "refresh plugin attributes");
            InstanceApi::new(&remote).plugin_list().await
        })
        .await
    }

    /// Commits fetched attributes and the next refresh time only after a successful fetch.
    async fn sync_with<F, Fut>(&self, force: bool, fetch: F) -> Result<(), BoxError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<PluginAttributes>, BoxError>>,
    {
        let mut next_sync = self.next_sync.lock().await;
        if !force && Instant::now() < *next_sync {
            return Ok(());
        }

        let attrs = fetch().await?;
        let attrs = attrs.into_iter().map(|attr| (attr.code.to_string(), attr)).collect();
        *self.attrs.write().await = attrs;
        *next_sync = Instant::now() + ATTR_SYNC_INTERVAL;
        Ok(())
    }

    /// Returns a snapshot of all cached plugin attributes.
    async fn values(&self) -> Vec<PluginAttributes> {
        self.attrs.read().await.values().cloned().collect()
    }
}

#[derive(Debug, Deserialize)]
struct WasmImageSchemaRequest {
    image_url: String,
    #[serde(default)]
    schema_path: Option<String>,
    #[serde(default)]
    oci_auth: Option<OciAuthConfig>,
}

async fn sync_attr_cache<B: Discovery>(backend: &B, refresh: bool) -> Result<(), BoxError> {
    ATTR_CACHE.get_or_init(PluginAttrCache::new).sync(backend, refresh).await
}

async fn get_schema_by_code<B: Discovery>(Path(plugin_code): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<Value>>, InternalError> {
    if let Some(remote) = backend.instances().await.map_err(InternalError)?.into_iter().next() {
        InstanceApi::new(&remote).schema(&plugin_code).await.map(Json).map_err(InternalError)
    } else {
        Ok(Json(None))
    }
}

async fn get_wasm_config_schema<B: Retrieve>(Query(id): Query<PluginInstanceId>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Value>, InternalError> {
    let Some(config) = backend.retrieve_plugin(&id).await.map_err(InternalError)? else {
        return Ok(Json(empty_schema()));
    };
    let Some(req) = wasm_schema_request_from_plugin_config(&config) else {
        return Ok(Json(empty_schema()));
    };
    load_wasm_schema(req).map(Json)
}

async fn preview_wasm_image_schema(Json(req): Json<WasmImageSchemaRequest>) -> Result<Json<Value>, InternalError> {
    load_wasm_schema(req).map(Json)
}

fn load_wasm_schema(req: WasmImageSchemaRequest) -> Result<Value, InternalError> {
    let image_url = req.image_url.trim();
    let schema_path = req.schema_path.as_deref().map(str::trim).filter(|v| !v.is_empty()).unwrap_or("schema.json");
    let bytes = match fetch_wasm_image_file_sync_with_auth(image_url, schema_path, req.oci_auth.as_ref()) {
        Ok(bytes) => bytes,
        Err(error) if is_missing_schema_file_error(&error) => return Ok(empty_schema()),
        Err(error) => return Err(InternalError::boxed(error)),
    };
    let value = parse_schema_bytes(&bytes).map_err(InternalError::boxed)?;
    Ok(value)
}

fn wasm_schema_request_from_plugin_config(config: &PluginConfig) -> Option<WasmImageSchemaRequest> {
    let spec = config.spec.as_object()?;
    let image_url = spec.get("image_url").or_else(|| spec.get("url")).and_then(Value::as_str)?.trim().to_string();
    if image_url.is_empty() {
        return None;
    }
    let schema_path = spec.get("schema_path").and_then(Value::as_str).map(ToOwned::to_owned);
    let oci_auth = spec.get("oci_auth").cloned().and_then(|value| serde_json::from_value(value).ok());
    Some(WasmImageSchemaRequest { image_url, schema_path, oci_auth })
}

fn empty_schema() -> Value {
    Value::Object(Default::default())
}

fn is_missing_schema_file_error(error: &WasmHostError) -> bool {
    let WasmHostError::Fetch(message) = error else {
        return false;
    };
    message.contains("does not contain `")
        || message.contains("No such file or directory")
        || message.contains("404 Not Found")
        || message.contains("does not contain Docker/OCI tar layers; cannot read `")
}

fn parse_schema_bytes(bytes: &[u8]) -> Result<Value, std::io::Error> {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(value) => return Ok(value),
        Err(json_err) => match serde_yaml::from_slice::<Value>(bytes) {
            Ok(value) => return Ok(value),
            Err(yaml_err) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("schema file is not valid JSON or YAML: json={json_err}; yaml={yaml_err}"),
                ));
            }
        },
    }
}
async fn get_list<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<String>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(PluginAttrCache::new).attrs.read().await.keys().cloned().collect()))
}
async fn get_attr_all<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<PluginAttributes>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(PluginAttrCache::new).values().await))
}
async fn get_attr<B: Discovery>(Path(plugin_code): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<PluginAttributes>>, InternalError> {
    sync_attr_cache(backend.as_ref(), false).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(PluginAttrCache::new).attrs.read().await.get(&plugin_code).cloned()))
}
async fn refresh_attr_cache<B: Discovery>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<PluginAttributes>>, InternalError> {
    sync_attr_cache(backend.as_ref(), true).await.map_err(InternalError)?;
    Ok(Json(ATTR_CACHE.get_or_init(PluginAttrCache::new).values().await))
}

pub fn router<B>() -> axum::Router<state::AppState<B>>
where
    B: Discovery + Retrieve + Send + Sync + 'static,
{
    Router::new()
        .route("/schema/{plugin_code}", get(get_schema_by_code::<B>))
        .route("/wasm/schema", get(get_wasm_config_schema::<B>))
        .route("/wasm/schema/preview", post(preview_wasm_image_schema))
        .route("/list", get(get_list::<B>))
        .route("/attr-all", get(get_attr_all::<B>))
        .route("/attr/{plugin_code}", get(get_attr::<B>))
        .route("/refresh", post(refresh_attr_cache::<B>))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_attr(code: &'static str) -> PluginAttributes {
        PluginAttributes {
            meta: Default::default(),
            mono: false,
            code: code.into(),
        }
    }

    #[tokio::test]
    async fn failed_refresh_preserves_cached_attributes_and_retry_time() {
        let cache = PluginAttrCache::new();
        cache.attrs.write().await.insert("existing".to_string(), plugin_attr("existing"));
        let retry_time = Instant::now() - Duration::from_secs(1);
        *cache.next_sync.lock().await = retry_time;

        let result = cache.sync_with(true, || async { Err::<Vec<PluginAttributes>, BoxError>("gateway unavailable".into()) }).await;

        assert!(result.is_err());
        assert_eq!(*cache.next_sync.lock().await, retry_time);
        assert_eq!(cache.attrs.read().await.keys().cloned().collect::<Vec<_>>(), vec!["existing"]);
    }

    #[tokio::test]
    async fn successful_refresh_replaces_cached_attributes_and_advances_retry_time() {
        let cache = PluginAttrCache::new();
        cache.attrs.write().await.insert("existing".to_string(), plugin_attr("existing"));
        let refresh_started = Instant::now();

        cache.sync_with(true, || async { Ok::<_, BoxError>(vec![plugin_attr("hai-auth"), plugin_attr("hai-asset")]) }).await.expect("refresh should succeed");

        assert!(*cache.next_sync.lock().await > refresh_started);
        let attrs = cache.attrs.read().await;
        assert!(!attrs.contains_key("existing"));
        assert!(attrs.contains_key("hai-auth"));
        assert!(attrs.contains_key("hai-asset"));
    }

    #[test]
    fn treats_absent_schema_file_as_empty_schema() {
        assert!(is_missing_schema_file_error(&WasmHostError::Fetch(
            "OCI image filesystem does not contain `schema.json`".to_string()
        )));
        assert!(is_missing_schema_file_error(&WasmHostError::Fetch(
            "OCI image does not contain Docker/OCI tar layers; cannot read `schema.json` from image filesystem".to_string()
        )));
        assert_eq!(empty_schema(), serde_json::json!({}));
    }

    #[test]
    fn keeps_registry_and_parse_failures_visible() {
        assert!(!is_missing_schema_file_error(&WasmHostError::Fetch(
            "GET https://registry.example.com/v2/: 401 Unauthorized".to_string()
        )));
        assert!(parse_schema_bytes(b"enabled: true").is_ok());
        assert!(parse_schema_bytes(b"[unclosed").is_err());
    }

    #[test]
    fn extracts_schema_request_from_saved_wasm_plugin_config() {
        let config = PluginConfig {
            id: PluginInstanceId::from_file_stem("wasm.hai-mix-process"),
            spec: serde_json::json!({
                "image_url": "oci+http://localhost:5001/hai-process-mix:dev",
                "schema_path": "schema.json",
                "oci_auth": {
                    "registry": "localhost:5001",
                    "username": "user",
                    "password": "pass"
                }
            }),
        };

        let req = wasm_schema_request_from_plugin_config(&config).expect("schema request");
        assert_eq!(req.image_url, "oci+http://localhost:5001/hai-process-mix:dev");
        assert_eq!(req.schema_path.as_deref(), Some("schema.json"));
        assert_eq!(req.oci_auth.as_ref().and_then(|auth| auth.registry.as_deref()), Some("localhost:5001"));
    }
}
