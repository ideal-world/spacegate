use spacegate_config::service::ListenEvent;
use spacegate_ext_axum::axum::{self, http, routing::post, Extension, Json, Router};
use tokio::sync::mpsc::Sender;
#[derive(Debug, Clone)]
pub struct App {
    pub listen_event_tx: Sender<ListenEvent>,
}
/// Axum Api Router
pub fn shell_routers(router: Router) -> Router {
    router.nest("/control", control_routes()).route("/health", axum::routing::get(axum::Json(true))).fallback(axum::routing::any(axum::response::Html(
        axum::body::Bytes::from_static(include_bytes!("./axum/static_resource/web-server-index.html")),
    )))
}

pub fn control_routes() -> Router {
    Router::new().route("push_event", post(event))
}

pub struct HttpEventListener {}

pub async fn event(state: Extension<App>, event: Json<ListenEvent>) -> Result<(), http::StatusCode> {
    let send_result = state.listen_event_tx.send(event.0).await;
    if let Err(_e) = send_result {
        Err(http::StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        Ok(())
    }
}
