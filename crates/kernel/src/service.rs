use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use futures_util::future::BoxFuture;
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_rustls::rustls;

use crate::{
    extension::{EnterTime, PeerAddr, Reflect},
    utils::with_length_or_chunked,
    ArcHyperService, BoxResult, SgBody,
};

pub mod http_route;

pub mod http_gateway;

pub trait TcpService: 'static + Send + Sync {
    fn protocol_name(&self) -> &str;
    fn sniff_peek_size(&self) -> usize;
    fn sniff(&self, peek_buf: &[u8]) -> bool;
    fn handle(&self, stream: TcpStream, peer: SocketAddr) -> BoxFuture<'static, BoxResult<()>>;
}
type ConnectionBuilder = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

#[derive(Debug)]
pub struct Http {
    inner_service: ArcHyperService,
    connection_builder: ConnectionBuilder,
}

impl Http {
    pub fn new(service: ArcHyperService) -> Self {
        Self {
            inner_service: service,
            connection_builder: ConnectionBuilder::new(Default::default()),
        }
    }
}

impl TcpService for Http {
    fn protocol_name(&self) -> &str {
        "http"
    }
    fn sniff_peek_size(&self) -> usize {
        14
    }
    fn sniff(&self, peeked: &[u8]) -> bool {
        peeked.starts_with(b"GET")
            || peeked.starts_with(b"HEAD")
            || peeked.starts_with(b"POST")
            || peeked.starts_with(b"PUT")
            || peeked.starts_with(b"DELETE")
            || peeked.starts_with(b"CONNECT")
            || peeked.starts_with(b"OPTIONS")
            || peeked.starts_with(b"TRACE")
            || peeked.starts_with(b"PATCH")
            || peeked.starts_with(b"PRI * HTTP/2.0")
    }
    fn handle(&self, stream: TcpStream, peer: SocketAddr) -> BoxFuture<'static, BoxResult<()>> {
        let io = TokioIo::new(stream);
        let service = HyperServiceAdapter::new(self.inner_service.clone(), peer);
        let builder = self.connection_builder.clone();
        Box::pin(async move {
            let conn = builder.serve_connection_with_upgrades(io, service);
            conn.await
        })
    }
}
#[derive(Debug)]
pub struct Https {
    inner_service: ArcHyperService,
    tls_config: Arc<rustls::ServerConfig>,
    connection_builder: ConnectionBuilder,
}

impl Https {
    pub fn new(service: ArcHyperService, tls_config: rustls::ServerConfig) -> Self {
        Self {
            inner_service: service,
            tls_config: Arc::new(tls_config),
            connection_builder: ConnectionBuilder::new(Default::default()),
        }
    }
}

impl TcpService for Https {
    fn protocol_name(&self) -> &str {
        "https"
    }
    fn sniff_peek_size(&self) -> usize {
        5
    }
    fn sniff(&self, peeked: &[u8]) -> bool {
        peeked.starts_with(b"\x16\x03")
    }
    fn handle(&self, stream: TcpStream, peer: SocketAddr) -> BoxFuture<'static, BoxResult<()>> {
        let service = HyperServiceAdapter::new(self.inner_service.clone(), peer);
        let builder = self.connection_builder.clone();
        let connector = tokio_rustls::TlsAcceptor::from(self.tls_config.clone());
        Box::pin(async move {
            let accepted = connector.accept(stream).await?;
            let conn = builder.serve_connection_with_upgrades(TokioIo::new(accepted), service);
            conn.await
        })
    }
}

#[derive(Clone, Debug)]
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
        // here, the service are likely to be a `ArcHyperService` so it's ok
        // but if underlying service is big, it will be expensive to clone.
        // especially the router is big and the too many plugins are installed.
        // so we should avoid that
        let enter_time = EnterTime::new();
        let service = self.service.clone();
        let mut req = req.map(SgBody::new);
        let mut reflect = Reflect::default();
        // let method = req.method().clone();
        reflect.insert(enter_time);
        req.extensions_mut().insert(reflect);
        req.extensions_mut().insert(PeerAddr(self.peer));
        req.extensions_mut().insert(enter_time);
        Box::pin(async move {
            let resp = service.call(req).await.expect("infallible");
            // if method != hyper::Method::HEAD && method != hyper::Method::OPTIONS && method != hyper::Method::CONNECT {
            //     with_length_or_chunked(&mut resp);
            // }
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

impl ArcHyperService {
    pub fn http(self) -> Http {
        Http::new(self)
    }
    pub fn https(self, tls_config: rustls::ServerConfig) -> Https {
        Https::new(self, tls_config)
    }
}
