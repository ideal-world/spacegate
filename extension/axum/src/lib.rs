use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::OnceLock;

pub use axum;
use axum::Router;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const GLOBAL_SERVER_PORT: u16 = 9876;
const GLOBAL_SERVER_HOST: IpAddr = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
#[derive(Debug)]
pub struct AxumServer {
    pub port: u16,
    pub host: IpAddr,
    pub router: Router,
    pub cancel_token: CancellationToken,
    handle: Option<JoinHandle<Result<(), std::io::Error>>>,
}

impl Default for AxumServer {
    fn default() -> Self {
        Self {
            port: GLOBAL_SERVER_PORT,
            host: GLOBAL_SERVER_HOST,
            router: Default::default(),
            cancel_token: Default::default(),
            handle: Default::default(),
        }
    }
}

impl AxumServer {
    pub fn global() -> Arc<RwLock<AxumServer>> {
        static GLOBAL: OnceLock<Arc<RwLock<AxumServer>>> = OnceLock::new();
        GLOBAL.get_or_init(Default::default).clone()
    }
    pub fn new(port: u16, host: IpAddr, cancel_token: CancellationToken) -> Self {
        Self {
            port,
            host,
            cancel_token,
            router: Router::new(),
            handle: None,
        }
    }
    pub async fn start(&mut self) -> Result<(), std::io::Error> {
        let _shutdown_result = self.shutdown().await;
        let socket_addr = SocketAddr::new(self.host, self.port);
        let tcp_listener = tokio::net::TcpListener::bind(socket_addr).await?;
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
