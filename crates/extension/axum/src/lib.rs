use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::OnceLock;

pub use axum;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{BoxError, Router};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
/// Default port for the global server
const GLOBAL_SERVER_PORT: u16 = 9876;
/// Default host for the global server
const GLOBAL_SERVER_HOST: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
/// Default bind to [::]:9876
const GLOBAL_SERVER_BIND: SocketAddr = SocketAddr::new(GLOBAL_SERVER_HOST, GLOBAL_SERVER_PORT);
#[derive(Debug)]
struct AxumServerInner {
    pub bind: SocketAddr,
    pub router: Router,
    pub cancel_token: CancellationToken,
    handle: Option<JoinHandle<Result<(), std::io::Error>>>,
}

impl Default for AxumServerInner {
    fn default() -> Self {
        Self {
            bind: GLOBAL_SERVER_BIND,
            router: Default::default(),
            cancel_token: Default::default(),
            handle: Default::default(),
        }
    }
}

/// Global axum http server for spacegate and its plugins.
///
/// # Usage
/// ```
/// # use spacegate_ext_axum::GlobalAxumServer;
/// let server = GlobalAxumServer::default();
/// ```
#[derive(Debug, Clone)]
pub struct GlobalAxumServer(Arc<RwLock<AxumServerInner>>);

impl Default for GlobalAxumServer {
    fn default() -> Self {
        Self(AxumServerInner::global())
    }
}

impl GlobalAxumServer {
    /// Set the bind address for the server. If the server is already running, new bind address will take effect after restart.
    pub async fn set_bind<A>(&self, socket_addr: A)
    where
        A: Into<SocketAddr>,
    {
        let socket_addr = socket_addr.into();
        let mut wg = self.0.write().await;
        wg.bind = socket_addr;
    }

    /// Get the bind address of the server.
    pub async fn get_bind(&self) -> SocketAddr {
        let wg = self.0.read().await;
        wg.bind
    }

    /// Set the cancellation token for the server.
    pub async fn set_cancellation(&self, token: CancellationToken) {
        let mut wg = self.0.write().await;
        wg.cancel_token = token;
    }

    /// Modify the router with the given closure.
    pub async fn modify_router<M>(&self, modify: M)
    where
        M: FnOnce(Router) -> Router,
    {
        let mut wg = self.0.write().await;
        let mut swap_out = Router::default();
        std::mem::swap(&mut swap_out, &mut wg.router);
        wg.router = (modify)(swap_out)
    }

    /// Start the server, if the server is already running, it will be restarted.
    pub async fn start(&self) -> Result<(), std::io::Error> {
        let mut wg = self.0.write().await;
        wg.start().await
    }

    /// Shutdown the server.
    pub async fn shutdown(&self) -> Result<(), std::io::Error> {
        let mut wg = self.0.write().await;
        wg.shutdown().await
    }
}

impl AxumServerInner {
    pub fn global() -> Arc<RwLock<AxumServerInner>> {
        static GLOBAL: OnceLock<Arc<RwLock<AxumServerInner>>> = OnceLock::new();
        GLOBAL.get_or_init(Default::default).clone()
    }
    pub async fn start(&mut self) -> Result<(), std::io::Error> {
        let _shutdown_result = self.shutdown().await;
        let tcp_listener = tokio::net::TcpListener::bind(self.bind).await?;
        let cancel = self.cancel_token.clone();
        let router = self.router.clone();
        let task = tokio::spawn(async move { axum::serve(tcp_listener, router).with_graceful_shutdown(cancel.cancelled_owned()).await });
        self.handle = Some(task);
        Ok(())
    }
    pub async fn shutdown(&mut self) -> Result<(), std::io::Error> {
        if let Some(handle) = self.handle.take() {
            self.cancel_token.cancel();
            handle.await.expect("tokio task join error")
        } else {
            Ok(())
        }
    }
}

pub struct InternalError {
    reason: BoxError,
}

impl IntoResponse for InternalError {
    fn into_response(self) -> Response {
        let body = axum::body::Body::from(format!("Internal error: {}", self.reason));
        Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(body).unwrap()
    }
}

impl<E> From<E> for InternalError
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(e: E) -> Self {
        Self { reason: Box::new(e) }
    }
}
