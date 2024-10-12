use futures_util::future::BoxFuture;
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::{self, TokioIo};

use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::{
    extension::{EnterTime, PeerAddr, Reflect},
    utils::with_length_or_chunked,
    BoxError, SgBody,
};

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Clone)]
pub struct SgListen<S> {
    conn_builder: hyper_util::server::conn::auto::Builder<rt::TokioExecutor>,
    pub socket_addr: SocketAddr,
    pub service: S,
    pub tls_cfg: Option<Arc<rustls::ServerConfig>>,
    pub cancel_token: CancellationToken,
    pub listener_id: String,
}

impl<S> std::fmt::Debug for SgListen<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgListen")
            .field("socket_addr", &self.socket_addr)
            .field("tls_enabled", &self.tls_cfg.is_some())
            .field("listener_id", &self.listener_id)
            .finish_non_exhaustive()
    }
}

impl<S> SgListen<S> {
    /// we only have 65535 ports for a console, so it's a safe size
    pub fn new(socket_addr: SocketAddr, service: S, cancel_token: CancellationToken) -> Self {
        let listener_id = format!("{socket_addr}");
        Self {
            conn_builder: hyper_util::server::conn::auto::Builder::new(rt::TokioExecutor::new()),
            socket_addr,
            service,
            tls_cfg: None,
            cancel_token,
            listener_id,
        }
    }

    /// Set the TLS config for this listener.
    /// see [rustls::ServerConfig](https://docs.rs/rustls/latest/rustls/server/struct.ServerConfig.html)
    #[must_use]
    pub fn with_tls_config(mut self, tls_cfg: impl Into<Arc<rustls::ServerConfig>>) -> Self {
        self.tls_cfg = Some(tls_cfg.into());
        self
    }
}

#[derive(Clone)]
struct HyperServiceAdapter<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    service: S,
    peer: SocketAddr,
}
impl<S> HyperServiceAdapter<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    pub fn new(service: S, peer: SocketAddr) -> Self {
        Self { service, peer }
    }
}

impl<S> hyper::service::Service<Request<Incoming>> for HyperServiceAdapter<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        req.extensions_mut().insert(self.peer);
        // here we will clone underlying service,
        // so it's important that underlying service is cheap to clone.
        // here, the service are likely to be a `ArcHyperService` so it's ok
        // but if underlying service is big, it will be expensive to clone.
        // especially the router is big and the too many plugins are installed.
        // so we should avoid that
        let enter_time = EnterTime::new();
        let service = self.service.clone();
        let mut req = req.map(SgBody::new);
        let mut reflect = Reflect::default();
        reflect.insert(enter_time);
        req.extensions_mut().insert(reflect);
        req.extensions_mut().insert(PeerAddr(self.peer));
        req.extensions_mut().insert(enter_time);

        Box::pin(async move {
            let mut resp = service.call(req).await.expect("infallible");
            with_length_or_chunked(&mut resp);
            let status = resp.status();
            if status.is_server_error() {
                tracing::warn!(status = ?status, headers = ?resp.headers(), "server error response");
            } else if status.is_client_error() {
                tracing::debug!(status = ?status, headers = ?resp.headers(), "client error response");
            } else if status.is_success() {
                tracing::trace!(status = ?status, headers = ?resp.headers(), "success response");
            }
            tracing::trace!(latency = ?enter_time.elapsed(), "request finished");
            Ok(resp)
        })
    }
}

impl<S> SgListen<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    #[instrument(skip(stream, service, tls_cfg, conn_builder))]
    async fn accept(
        conn_builder: hyper_util::server::conn::auto::Builder<rt::TokioExecutor>,
        stream: TcpStream,
        peer_addr: SocketAddr,
        tls_cfg: Option<Arc<rustls::ServerConfig>>,
        service: S,
    ) {
        // identify protocol
        let mut protocol_buffer = [0; 8];
        let Ok(_) = stream.peek(&mut protocol_buffer).await else { return };
        if protocol_buffer.starts_with(b"SSH-") {
            // stream is ssh
        } else if protocol_buffer.starts_with(b"\x16\x03") {
            // stream is http
        } else {
            // otherwise stream is http
        }
        tracing::debug!("[Sg.Listen] Accepted connection");
        let service = HyperServiceAdapter::new(service, peer_addr);
        let conn_result = if let Some(tls_cfg) = tls_cfg {
            let connector = tokio_rustls::TlsAcceptor::from(tls_cfg);
            let Ok(accepted) = connector.accept(stream).await.inspect_err(|e| tracing::warn!("[Sg.Listen] Tls connect error: {}", e)) else {
                return;
            };
            let io = TokioIo::new(accepted);
            let conn = conn_builder.serve_connection_with_upgrades(io, service);
            conn.await
        } else {
            let io = TokioIo::new(stream);
            let conn = conn_builder.serve_connection_with_upgrades(io, service);
            conn.await
        };
        if let Err(e) = conn_result {
            tracing::warn!("[Sg.Listen] Connection closed with error {e}")
        } else {
            tracing::debug!("[Sg.Listen] Connection closed");
        }
    }
    #[instrument()]
    pub async fn listen(self) -> Result<(), BoxError> {
        tracing::debug!("[Sg.Listen] start binding...");
        let listener = tokio::net::TcpListener::bind(self.socket_addr).await?;
        let cancel_token = self.cancel_token;
        tracing::debug!("[Sg.Listen] start listening...");
        loop {
            let accepted = tokio::select! {
                () = cancel_token.cancelled() => {
                    tracing::warn!("[Sg.Listen] cancelled");
                    return Ok(());
                },
                accepted = listener.accept() => accepted
            };
            match accepted {
                Ok((stream, peer_addr)) => {
                    let tls_cfg = self.tls_cfg.clone();
                    let service = self.service.clone();
                    let builder = self.conn_builder.clone();
                    tokio::spawn(Self::accept(builder, stream, peer_addr, tls_cfg, service));
                }
                Err(e) => {
                    tracing::warn!("[Sg.Listen] Accept tcp connection error: {:?}", e);
                }
            }
        }
    }
}
