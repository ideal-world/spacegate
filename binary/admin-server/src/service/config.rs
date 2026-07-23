use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use spacegate_config::{
    model::{SgGateway, SgRoute},
    service::*,
    BoxError, Config, ConfigItem, PluginConfig, PluginInstanceId,
};
use std::collections::BTreeMap;

use crate::{
    error::InternalError,
    state::{self, AppState},
    Backend,
};

/// 插件创建和更新的查询参数，实例 ID 与展示名称分开管理。
#[derive(Debug, Deserialize)]
struct PluginUpsertQuery {
    #[serde(flatten)]
    id: PluginInstanceId,
    #[serde(default)]
    display_name: Option<String>,
}

impl PluginUpsertQuery {
    /// 将查询参数和原始运行时 spec 合并为完整插件管理配置。
    fn into_config(self, spec: Value) -> PluginConfig {
        PluginConfig {
            id: self.id,
            display_name: normalize_plugin_display_name(self.display_name),
            spec,
        }
    }
}

/**********************************************
                       GET
**********************************************/

async fn get_config_item_gateway<B: Retrieve>(
    Path(gateway_name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Option<SgGateway>>, InternalError<BoxError>> {
    backend.retrieve_config_item_gateway(&gateway_name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_route<B: Retrieve>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Option<SgRoute>>, InternalError<BoxError>> {
    backend.retrieve_config_item_route(&name, &route_name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_route_names<B: Retrieve>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Vec<String>>, InternalError<BoxError>> {
    backend.retrieve_config_item_route_names(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_all_routes<B: Retrieve>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<BTreeMap<String, SgRoute>>, InternalError<BoxError>> {
    backend.retrieve_config_item_all_routes(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_item<B: Retrieve>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<ConfigItem>>, InternalError<BoxError>> {
    backend.retrieve_config_item(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_names<B: Retrieve>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<String>>, InternalError<BoxError>> {
    backend.retrieve_config_names().await.map(Json).map_err(InternalError)
}
async fn get_config<B: Retrieve>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Config>, InternalError<BoxError>> {
    backend.retrieve_config().await.map(Json).map_err(InternalError)
}
async fn get_config_all_plugin<B: Retrieve>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<PluginConfig>>, InternalError<BoxError>> {
    backend.retrieve_all_plugins().await.map(Json).map_err(InternalError)
}
async fn get_config_plugin<B: Retrieve>(
    Query(id): Query<PluginInstanceId>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Option<PluginConfig>>, InternalError<BoxError>> {
    backend.retrieve_plugin(&id).await.map(Json).map_err(InternalError)
}

async fn get_config_plugins_by_code<B: Retrieve>(
    Path(code): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Vec<PluginConfig>>, InternalError<BoxError>> {
    backend.retrieve_plugins_by_code(&code).await.map(Json).map_err(InternalError)
}
/**********************************************
                       POST
**********************************************/
async fn post_config_item<B: Create>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(config_item): Json<ConfigItem>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_config_item(&name, config_item).await.map_err(InternalError)
}
async fn post_config<B: Create>(State(AppState { backend, .. }): State<AppState<B>>, Json(config): Json<Config>) -> Result<(), InternalError<BoxError>> {
    backend.create_config(config).await.map_err(InternalError)
}
async fn post_config_item_gateway<B: Create>(
    Path(gateway_name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(gateway): Json<SgGateway>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_config_item_gateway(&gateway_name, gateway).await.map_err(InternalError)
}
async fn post_config_item_route<B: Create>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(route): Json<SgRoute>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_config_item_route(&name, &route_name, route).await.map_err(InternalError)
}
async fn post_config_plugin<B: Create>(
    Query(query): Query<PluginUpsertQuery>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(spec): Json<Value>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_plugin(query.into_config(spec)).await.map_err(InternalError)
}
/**********************************************
                       PUT
**********************************************/
async fn put_config_item_gateway<B: Update>(
    Path(gateway_name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(gateway): Json<SgGateway>,
) -> Result<(), InternalError<BoxError>> {
    backend.update_config_item_gateway(&gateway_name, gateway).await.map_err(InternalError)
}

async fn put_config_item_route<B: Update>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(route): Json<SgRoute>,
) -> Result<(), InternalError<BoxError>> {
    backend.update_config_item_route(&name, &route_name, route).await.map_err(InternalError)
}

async fn put_config_item<B: Update>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(config_item): Json<ConfigItem>,
) -> Result<(), InternalError<BoxError>> {
    backend.update_config_item(&name, config_item).await.map_err(InternalError)
}

async fn put_config<B: Update>(State(AppState { backend, .. }): State<AppState<B>>, Json(config): Json<Config>) -> Result<(), InternalError<BoxError>> {
    backend.update_config(config).await.map_err(InternalError)
}

async fn put_config_plugin<B: Update>(
    Query(query): Query<PluginUpsertQuery>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(spec): Json<Value>,
) -> Result<(), InternalError<BoxError>> {
    backend.update_plugin(query.into_config(spec)).await.map_err(InternalError)
}
/**********************************************
                       DELETE
**********************************************/

async fn delete_config_item_gateway<B: Delete>(Path(gateway_name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>> {
    backend.delete_config_item_gateway(&gateway_name).await.map_err(InternalError)
}

async fn delete_config_item_route<B: Delete>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<(), InternalError<BoxError>> {
    backend.delete_config_item_route(&name, &route_name).await.map_err(InternalError)
}

async fn delete_config_item<B: Delete + Retrieve>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>> {
    backend.delete_config_item(&name).await.map_err(InternalError)
}

async fn delete_config_item_all_routes<B: Delete + Retrieve>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>> {
    backend.delete_config_item_all_routes(&name).await.map_err(InternalError)
}

async fn delete_config_plugin<B: Delete + Retrieve>(
    Query(id): Query<PluginInstanceId>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<(), InternalError<BoxError>> {
    backend.delete_plugin(&id).await.map_err(InternalError)
}

// router
pub fn router<B>() -> axum::Router<state::AppState<B>>
where
    B: Backend + Create + Retrieve + Update + Delete + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(get_config::<B>).post(post_config::<B>).put(put_config::<B>))
        .route("/names", get(get_config_names::<B>))
        .nest(
            "/item",
            Router::new()
                .route(
                    "/{name}",
                    get(get_config_item::<B>).post(post_config_item::<B>).put(put_config_item::<B>).delete(delete_config_item::<B>),
                )
                .route(
                    "/{name}/route/item/{route}",
                    get(get_config_item_route::<B>).post(post_config_item_route::<B>).put(put_config_item_route::<B>).delete(delete_config_item_route::<B>),
                )
                .route("/{name}/route/all", get(get_config_item_all_routes::<B>).delete(delete_config_item_all_routes))
                .route("/{name}/route/names", get(get_config_item_route_names::<B>))
                .route(
                    "/{name}/gateway",
                    get(get_config_item_gateway::<B>).post(post_config_item_gateway::<B>).put(put_config_item_gateway::<B>).delete(delete_config_item_gateway::<B>),
                ),
        )
        .route(
            "/plugin",
            get(get_config_plugin).delete(delete_config_plugin).put(put_config_plugin).post(post_config_plugin),
        )
        .route("/plugin-all", get(get_config_all_plugin))
        .route("/plugins/{code}", get(get_config_plugins_by_code))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use spacegate_config::PluginInstanceName;

    use super::PluginUpsertQuery;

    #[test]
    fn plugin_upsert_query_accepts_legacy_id_without_display_name() {
        let query: PluginUpsertQuery = serde_urlencoded::from_str("code=hai-auth&kind=named&name=auth-a1").unwrap();

        assert_eq!(query.id.code, "hai-auth");
        assert_eq!(query.id.name, PluginInstanceName::named("auth-a1"));
        assert_eq!(query.display_name, None);
    }

    #[test]
    fn plugin_upsert_query_decodes_and_normalizes_display_name() {
        let query: PluginUpsertQuery = serde_urlencoded::from_str("code=hai-auth&kind=named&name=auth-a1&display_name=+%E7%94%9F%E4%BA%A7+%E9%89%B4%E6%9D%83+").unwrap();
        let config = query.into_config(json!({ "cache_url": "redis://redis:6379" }));

        assert_eq!(config.display_name.as_deref(), Some("生产 鉴权"));
        assert_eq!(config.spec, json!({ "cache_url": "redis://redis:6379" }));

        let query: PluginUpsertQuery = serde_urlencoded::from_str("code=hai-auth&kind=named&name=auth-a1&display_name=+++").unwrap();
        assert_eq!(query.into_config(json!({})).display_name, None);
    }
}
