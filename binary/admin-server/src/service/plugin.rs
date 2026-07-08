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
use std::{collections::HashMap, sync::OnceLock, time::Duration};
use tokio::{sync::RwLock, time::Instant};
use tracing::info;

static ATTR_CACHE: OnceLock<RwLock<HashMap<String, PluginAttributes>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct WasmImageSchemaRequest {
    image_url: String,
    #[serde(default)]
    schema_path: Option<String>,
    #[serde(default)]
    oci_auth: Option<OciAuthConfig>,
}

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
    B: Discovery + Retrieve + Send + Sync + 'static,
{
    Router::new()
        .route("/schema/{plugin_code}", get(get_schema_by_code::<B>))
        .route("/wasm/schema", get(get_wasm_config_schema::<B>))
        .route("/wasm/schema/preview", post(preview_wasm_image_schema))
        .route("/list", get(get_list::<B>))
        .route("/attr-all", get(get_attr_all::<B>))
        .route("/attr/{plugin_code}", get(get_attr::<B>))
}

#[cfg(test)]
mod tests {
    use super::*;

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
