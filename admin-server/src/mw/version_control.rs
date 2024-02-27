use std::sync::{atomic::AtomicU64, Arc};

use axum::{
    extract::{self, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::AppState;
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

pub async fn version_control<B>(State(state): State<AppState<B>>, request: extract::Request, next: Next) -> Response {
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
