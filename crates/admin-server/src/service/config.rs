use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;
use spacegate_config::{
    model::{SgGateway, SgHttpRoute},
    service::*,
    BoxError, Config, ConfigItem, PluginConfig, PluginInstanceId,
};
use std::collections::BTreeMap;

use crate::{
    error::InternalError,
    state::{self, AppState},
    Backend,
};

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
) -> Result<Json<Option<SgHttpRoute>>, InternalError<BoxError>> {
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
) -> Result<Json<BTreeMap<String, SgHttpRoute>>, InternalError<BoxError>> {
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
    Json(route): Json<SgHttpRoute>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_config_item_route(&name, &route_name, route).await.map_err(InternalError)
}
async fn post_config_plugin<B: Create>(
    Query(id): Query<PluginInstanceId>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(spec): Json<Value>,
) -> Result<(), InternalError<BoxError>> {
    backend.create_plugin(&id, spec).await.map_err(InternalError)
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
    Json(route): Json<SgHttpRoute>,
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
    Query(id): Query<PluginInstanceId>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(spec): Json<Value>,
) -> Result<(), InternalError<BoxError>> {
    backend.update_plugin(&id, spec).await.map_err(InternalError)
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

async fn delete_config_item<B: Delete>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>>
where
    B: Retrieve,
{
    backend.delete_config_item(&name).await.map_err(InternalError)
}

async fn delete_config_item_all_routes<B: Delete>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>>
where
    B: Retrieve,
{
    backend.delete_config_item_all_routes(&name).await.map_err(InternalError)
}

async fn delete_config_plugin<B: Delete>(Query(id): Query<PluginInstanceId>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<BoxError>> {
    backend.delete_plugin(&id).await.map_err(InternalError)
}

// router
pub fn router<B: Backend>() -> axum::Router<state::AppState<B>>
where
    B: Create + Retrieve + Update + Delete + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(get_config::<B>).post(post_config::<B>).put(put_config::<B>))
        .route("/names", get(get_config_names::<B>))
        .nest(
            "/item",
            Router::new()
                .route(
                    "/:name",
                    get(get_config_item::<B>).post(post_config_item::<B>).put(put_config_item::<B>).delete(delete_config_item::<B>),
                )
                .route(
                    "/:name/route/item/:route",
                    get(get_config_item_route::<B>).post(post_config_item_route::<B>).put(put_config_item_route::<B>).delete(delete_config_item_route::<B>),
                )
                .route("/:name/route/all", get(get_config_item_all_routes::<B>).delete(delete_config_item_all_routes))
                .route("/:name/route/names", get(get_config_item_route_names::<B>))
                .route(
                    "/:name/gateway",
                    get(get_config_item_gateway::<B>).post(post_config_item_gateway::<B>).put(put_config_item_gateway::<B>).delete(delete_config_item_gateway::<B>),
                ),
        )
        .route(
            "/plugin",
            get(get_config_plugin).delete(delete_config_plugin).put(put_config_plugin).post(post_config_plugin),
        )
        .route("/plugin-all", get(get_config_all_plugin))
}
