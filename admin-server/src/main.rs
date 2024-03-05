use std::{collections::BTreeMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware::{self},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use spacegate_config::{
    model::{SgGateway, SgHttpRoute},
    service::{self, *},
    BoxError, Config, ConfigItem,
};
pub mod clap;
pub mod mw;

pub trait Backend: Create + Retrieve + Update + Delete + Send + Sync + 'static {}

impl<T> Backend for T where T: Create + Retrieve + Update + Delete + Send + Sync + 'static {}

#[derive(Debug)]
pub struct AppState<B> {
    pub backend: Arc<B>,
    pub version: mw::version_control::Version,
}

impl<B> Clone for AppState<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            version: self.version.clone(),
        }
    }
}

pub struct InternalError<E>(pub E);
impl IntoResponse for InternalError<BoxError> {
    fn into_response(self) -> Response {
        let body = axum::body::Body::from(format!("Internal error: {}", self.0));
        Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(body).unwrap()
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();
    let args = <crate::clap::Args as ::clap::Parser>::parse();
    tracing::info!("server started with args: {:?}", args);
    let addr = SocketAddr::new(args.host, args.port);
    let app = match args.config {
        clap::ConfigBackend::File(path) => {
            let backend = service::backend::fs::Fs::new(path, config_format::Json::default());
            create_app(backend)
        }
        clap::ConfigBackend::K8s(ns) => {
            let backend = service::backend::k8s::K8s::with_default_client(ns).await?;
            create_app(backend)
        }
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.layer(tower_http::trace::TraceLayer::new_for_http()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

pub fn create_app<B>(backend: B) -> Router<()>
where
    B: Create + Retrieve + Update + Delete + Send + Sync + 'static,
{
    let state = AppState {
        backend: Arc::new(backend),
        version: mw::version_control::Version::new(),
    };
    Router::new().nest(
        "/config",
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
            .layer(middleware::from_fn_with_state(state.clone(), mw::version_control::version_control))
            .with_state(state),
    )
}