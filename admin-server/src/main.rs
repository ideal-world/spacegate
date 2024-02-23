use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
};

use axum::{
    extract::{self, Path, State},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use spacegate_config::{
    service, config_format,
    create::Create,
    delete::Delete,
    model::{SgGateway, SgHttpRoute},
    retrieve::Retrieve,
    update::Update,
    Config, ConfigItem,
};
pub mod clap;
pub trait Backend<E>: Create<Error = E> + Retrieve<Error = E> + Update<Error = E> + Delete<Error = E> + Send + Sync + 'static {}

impl<T, E> Backend<E> for T where T: Create<Error = E> + Retrieve<Error = E> + Update<Error = E> + Delete<Error = E> + Send + Sync + 'static {}
#[derive(Debug)]
pub struct AppState<B> {
    pub backend: Arc<B>,
    pub version: Version,
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
impl<E> IntoResponse for InternalError<E>
where
    E: std::error::Error,
{
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
) -> Result<Json<Option<SgGateway>>, InternalError<B::Error>> {
    backend.retrieve_config_item_gateway(&gateway_name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_route<B: Retrieve>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Option<SgHttpRoute>>, InternalError<B::Error>> {
    backend.retrieve_config_item_route(&name, &route_name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_route_names<B: Retrieve>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<Vec<String>>, InternalError<B::Error>> {
    backend.retrieve_config_item_route_names(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_item_all_routes<B: Retrieve>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<Json<BTreeMap<String, SgHttpRoute>>, InternalError<B::Error>> {
    backend.retrieve_config_item_all_routes(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_item<B: Retrieve>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Option<ConfigItem>>, InternalError<B::Error>> {
    backend.retrieve_config_item(&name).await.map(Json).map_err(InternalError)
}
async fn get_config_names<B: Retrieve>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Vec<String>>, InternalError<B::Error>> {
    backend.retrieve_config_names().await.map(Json).map_err(InternalError)
}
async fn get_config<B: Retrieve>(State(AppState { backend, .. }): State<AppState<B>>) -> Result<Json<Config>, InternalError<B::Error>> {
    backend.retrieve_config().await.map(Json).map_err(InternalError)
}

/**********************************************
                       POST
**********************************************/
async fn post_config_item<B: Create>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(config_item): Json<ConfigItem>,
) -> Result<(), InternalError<B::Error>> {
    backend.create_config_item(&name, &config_item).await.map_err(InternalError)
}
async fn post_config<B: Create>(State(AppState { backend, .. }): State<AppState<B>>, Json(config): Json<Config>) -> Result<(), InternalError<B::Error>> {
    backend.create_config(&config).await.map_err(InternalError)
}
async fn post_config_item_gateway<B: Create>(
    Path(gateway_name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(gateway): Json<SgGateway>,
) -> Result<(), InternalError<B::Error>> {
    backend.create_config_item_gateway(&gateway_name, &gateway).await.map_err(InternalError)
}
async fn post_config_item_route<B: Create>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(route): Json<SgHttpRoute>,
) -> Result<(), InternalError<B::Error>> {
    backend.create_config_item_route(&name, &route_name, &route).await.map_err(InternalError)
}

/**********************************************
                       PUT
**********************************************/
async fn put_config_item_gateway<B: Update>(
    Path(gateway_name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(gateway): Json<SgGateway>,
) -> Result<(), InternalError<B::Error>> {
    backend.update_config_item_gateway(&gateway_name, &gateway).await.map_err(InternalError)
}

async fn put_config_item_route<B: Update>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(route): Json<SgHttpRoute>,
) -> Result<(), InternalError<B::Error>> {
    backend.update_config_item_route(&name, &route_name, &route).await.map_err(InternalError)
}

async fn put_config_item<B: Update>(
    Path(name): Path<String>,
    State(AppState { backend, .. }): State<AppState<B>>,
    Json(config_item): Json<ConfigItem>,
) -> Result<(), InternalError<B::Error>> {
    backend.update_config_item(&name, &config_item).await.map_err(InternalError)
}

async fn put_config<B: Update>(State(AppState { backend, .. }): State<AppState<B>>, Json(config): Json<Config>) -> Result<(), InternalError<B::Error>> {
    backend.update_config(&config).await.map_err(InternalError)
}

/**********************************************
                       DELETE
**********************************************/

async fn delete_config_item_gateway<B: Delete>(Path(gateway_name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<B::Error>> {
    backend.delete_config_item_gateway(&gateway_name).await.map_err(InternalError)
}

async fn delete_config_item_route<B: Delete>(
    Path((name, route_name)): Path<(String, String)>,
    State(AppState { backend, .. }): State<AppState<B>>,
) -> Result<(), InternalError<B::Error>> {
    backend.delete_config_item_route(&name, &route_name).await.map_err(InternalError)
}

async fn delete_config_item<B: Delete>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<<B as Delete>::Error>>
where
    B: Retrieve,
    <B as Delete>::Error: From<<B as Retrieve>::Error>,
{
    backend.delete_config_item(&name).await.map_err(InternalError)
}

async fn delete_config_item_all_routes<B: Delete>(Path(name): Path<String>, State(AppState { backend, .. }): State<AppState<B>>) -> Result<(), InternalError<<B as Delete>::Error>>
where
    B: Retrieve,
    <B as Delete>::Error: From<<B as Retrieve>::Error>,
{
    backend.delete_config_item_all_routes(&name).await.map_err(InternalError)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = <crate::clap::Args as ::clap::Parser>::parse();
    let addr = SocketAddr::new(args.host, args.port);
    let app = match args.backend {
        clap::Backend::File(path) => {
            let backend = service::fs::Fs::new(path, config_format::Json::default());
            create_app(backend)
        }
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct Version {
    pub version: Arc<AtomicU64>,
}

impl Version {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&self) -> u64 {
        self.version.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn equal(&self, version: u64) -> bool {
        self.version.load(std::sync::atomic::Ordering::Relaxed) == version
    }
    pub fn fetch(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }
}

async fn version_control<B>(State(state): State<AppState<B>>, request: extract::Request, next: Next) -> Response {
    const CLIENT_HEADER: &str = "X-Client-Version";
    const SERVER_HEADER: &str = "X-Server-Version";
    // do something with `request`...
    let client_version = request.headers().get(CLIENT_HEADER).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or_default();
    let method = request.method().clone();
    if method == Method::DELETE || method == Method::POST || method == Method::PUT {
        if state.version.equal(client_version) {
            // up to date, update version
            state.version.update();
        } else {
            // out of date, tell client to update
            return Response::builder()
                .status(StatusCode::CONFLICT)
                .header(SERVER_HEADER, state.version.fetch())
                .body(axum::body::Body::empty())
                .expect("should be valid response");
        }
    }
    let version = state.version.fetch();
    let mut response = next.run(request).await;
    if method == Method::GET {
        response.headers_mut().insert(SERVER_HEADER, version.into());
    }
    response
}

pub fn create_app<B>(backend: B) -> Router<()>
where
    B: Create + Retrieve + Update + Delete + Send + Sync + 'static,
    <B as Delete>::Error: From<<B as Retrieve>::Error>,
{
    let state = AppState {
        backend: Arc::new(backend),
        version: Version::new(),
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
            .layer(middleware::from_fn_with_state(state.clone(), version_control))
            .with_state(state),
    )
}
