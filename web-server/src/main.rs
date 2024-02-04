use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use spacegate_config::{backend, config_format, retrieve::Retrieve, save::Save, Config, ConfigItem};

pub struct GetConfigItemService<B> {
    pub backend: B,
}

async fn get_config_item<B: Retrieve>(Path(name): Path<String>, State(backend): State<Arc<B>>) -> Result<Json<Option<ConfigItem>>, StatusCode> {
    backend.retrieve_config_item(&name).await.map(Json).map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn post_config_item<B: Save>(Path(name): Path<String>, State(backend): State<Arc<B>>, Json(config_item): Json<ConfigItem>) -> StatusCode {
    if backend.save_config_item(&name, &config_item).await.is_err() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

async fn get_config<B: Retrieve>(State(backend): State<Arc<B>>) -> Result<Json<Config>, StatusCode> {
    backend.retrieve_config().await.map(Json).map_err(|_e| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn post_config<B: Save>(State(backend): State<Arc<B>>, Json(config): Json<Config>) -> StatusCode {
    if backend.save_config(&config).await.is_err() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

#[tokio::main]
async fn main() {
    let backend = backend::fs::Fs::new("./config", config_format::Json::default());

    let app = create_app(backend);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
}

pub fn create_app<B>(backend: B) -> Router<Arc<B>>
where
    B: Save + Retrieve + Send + Sync + 'static,
{
    Router::new()
        .route("/config/:name", get(get_config_item::<B>).post(post_config_item::<B>))
        .route("/config", get(get_config::<B>).post(post_config::<B>))
        .layer(Extension(Arc::new(backend)))
}
