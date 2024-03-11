use crate::state::{self, AppState, PluginCode};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;

async fn get_schema_by_code<B>(Path(plugin_code): Path<String>, State(AppState { plugin_schemas: schemas, .. }): State<AppState<B>>) -> Json<Option<Value>> {
    Json::from(schemas.get(&PluginCode::plugin(plugin_code)).cloned())
}

async fn get_list<B>(State(AppState { plugin_schemas: schemas, .. }): State<AppState<B>>) -> Json<Vec<String>> {
    let schemas = schemas.keys().map(|name| name.to_string()).collect::<Vec<_>>();
    Json::from(schemas)
}

pub fn router<B>() -> axum::Router<state::AppState<B>>
where
    B: Send + Sync + 'static,
{
    Router::new().route("/schema/:plugin_code", get(get_schema_by_code::<B>)).route("/list", get(get_list::<B>))
}
