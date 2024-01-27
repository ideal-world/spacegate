use futures_util::future::BoxFuture;
use hyper::{body::Incoming, Request, Response, StatusCode};
use hyper_util::rt::{self, TokioIo};

use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tokio_util::sync::CancellationToken;
use tower::{buffer::Buffer, BoxError, ServiceExt};
use tracing::instrument;

use crate::{
    extension::{EnterTime, PeerAddr, Reflect},
    utils::with_length_or_chunked,
    SgBody,
};

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Clone)]
pub struct SgListen<S> {
    conn_builder: hyper_util::server::conn::auto::Builder<rt::TokioExecutor>,
    pub socket_addr: SocketAddr,
    pub service: S,
    pub tls_cfg: Option<Arc<rustls::ServerConfig>>,
    pub buffer_size: usize,
    pub cancel_token: CancellationToken,
    pub listener_id: String,
}

impl<S> std::fmt::Debug for SgListen<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgListen").field("socket_addr", &self.socket_addr).field("tls_enabled", &self.tls_cfg.is_some()).field("listener_id", &self.listener_id).finish()
    }
}

impl<S> SgListen<S> {
    /// we only have 65535 ports for a console, so it's a safe size
    pub const DEFAULT_BUFFER_SIZE: usize = 0x10000;
    pub fn new(socket_addr: SocketAddr, service: S, cancel_token: CancellationToken, id: impl Into<String>) -> Self {
        Self {
            conn_builder: hyper_util::server::conn::auto::Builder::new(rt::TokioExecutor::new()),
            socket_addr,
            service,
            tls_cfg: None,
            buffer_size: Self::DEFAULT_BUFFER_SIZE,
            cancel_token,
            listener_id: id.into(),
        }
    }

    /// Set the TLS config for this listener.
    /// see [rustls::ServerConfig](https://docs.rs/rustls/latest/rustls/server/struct.ServerConfig.html)
    pub fn with_tls_config(mut self, tls_cfg: impl Into<Arc<rustls::ServerConfig>>) -> Self {
        self.tls_cfg = Some(tls_cfg.into());
        self
    }

    /// # Choosing a buffer size
    ///
    /// The `buffer_size` should be lager than the maximal number of concurrent requests.
    ///
    /// However, a too large buffer size is unreasonable. Too many requests could wait for a long time for underlying service to process.
    pub fn buffer_size(mut self, buffer_size: usize) -> Self {
        self.buffer_size = buffer_size;
        self
    }
}

#[derive(Clone)]
pub struct HyperServiceAdapter<S>
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
        // here, the service are likely to be a `BoxHyperService`, if underlying service is big, it will be expensive to clone.
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
            let mut resp = match service.call(req).await {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!("buffer service call error: {:?}", e);
                    let error = e.to_string();
                    return Ok(Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(SgBody::full(error)).expect("constructing invalid response"));
                }
            };
            with_length_or_chunked(&mut resp);
            tracing::trace!(time_used = ?enter_time.elapsed(), "request finished");
            Ok(resp)
        })
    }
}

impl<S> SgListen<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    #[instrument(skip(stream, service, tls_cfg))]
    async fn accept(
        conn_builder: hyper_util::server::conn::auto::Builder<rt::TokioExecutor>,
        stream: TcpStream,
        peer_addr: SocketAddr,
        tls_cfg: Option<Arc<rustls::ServerConfig>>,
        cancel_token: CancellationToken,
        service: S,
    ) -> Result<(), BoxError> {
        tracing::debug!("[Sg.Listen] Accepted connection");
        let service = HyperServiceAdapter::new(service, peer_addr);
        match tls_cfg {
            Some(tls_cfg) => {
                let connector = tokio_rustls::TlsAcceptor::from(tls_cfg);
                let accepted = connector.accept(stream).await?;
                let io = TokioIo::new(accepted);
                let conn = conn_builder.serve_connection(io, service);
                conn.await?;
            }
            None => {
                let io = TokioIo::new(stream);
                let conn = conn_builder.serve_connection(io, service);
                conn.await?;
            }
        }
        tracing::debug!("[Sg.Listen] Connection closed");
        Ok(())
    }
    #[instrument()]
    pub async fn listen(self) -> Result<(), BoxError> {
        tracing::debug!("[Sg.Listen] start binding...");
        let listener = tokio::net::TcpListener::bind(self.socket_addr).await?;
        let cancel_token = self.cancel_token;
        tracing::debug!("[Sg.Listen] start listening...");
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    tracing::warn!("[Sg.Listen] cancelled");
                    return Ok(());
                },
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer_addr)) => {
                            let tls_cfg = self.tls_cfg.clone();
                            let service = self.service.clone();
                            let builder = self.conn_builder.clone();
                            let cancel_token = cancel_token.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::accept(builder, stream, peer_addr, tls_cfg, cancel_token, service).await {
                                    tracing::warn!("[Sg.Listen] Accept stream error: {:?}", e);
                                }
                            });
                        },
                        Err(e) => {
                            tracing::warn!("[Sg.Listen] Accept tcp connection error: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}
