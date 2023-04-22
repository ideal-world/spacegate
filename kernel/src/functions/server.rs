use std::{
    collections::HashMap,
    future::Future,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
};

use crate::config::gateway_dto::{SgGateway, SgProtocol};
use core::task::{Context, Poll};
use http::{Request, Response};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Server, StatusCode};
use lazy_static::lazy_static;
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::pin::Pin;
use std::sync::Arc;
use std::vec::Vec;
use std::{io, sync};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log,
    tokio::{self, sync::watch::Sender},
};
use tardis::{
    futures_util::{ready, FutureExt},
    tokio::sync::Mutex,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
}

pub async fn startup(gateway_conf: SgGateway) -> TardisResult<()> {
    if gateway_conf.listeners.is_empty() {
        return Err(TardisError::bad_request("[SG.server] Missing Listeners", ""));
    }
    if gateway_conf.listeners.iter().any(|l| l.protocol != SgProtocol::Http) {
        return Err(TardisError::bad_request("[SG.server] Non-Http protocols are not supported yet", ""));
    }
    let (shutdown_tx, _) = tokio::sync::watch::channel(());

    let mut serv_futures = Vec::new();
    for listener in gateway_conf.listeners {
        let ip = listener.ip.as_deref().unwrap_or("0.0.0.0");
        let addr = if ip.contains('.') {
            let ip: Ipv4Addr = ip.parse().map_err(|error| TardisError::bad_request(&format!("[SG.server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V4(ip), listener.port)
        } else {
            let ip: Ipv6Addr = ip.parse().map_err(|error| TardisError::bad_request(&format!("[SG.server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V6(ip), listener.port)
        };

        let mut shutdown_rx = shutdown_tx.subscribe();

        if let Some(tls) = listener.tls {
            let tls_cfg = {
                let cert = Certificate(tls.cert.as_bytes().to_vec());
                let key = PrivateKey(tls.key.as_bytes().to_vec());
                let mut cfg = rustls::ServerConfig::builder()
                    .with_safe_defaults()
                    .with_no_client_auth()
                    .with_single_cert(vec![cert], key)
                    .map_err(|error| TardisError::bad_request(&format!("[SG.server] Tls not legal: {error}"), ""))?;
                cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                sync::Arc::new(cfg)
            };
            let incoming = AddrIncoming::bind(&addr).map_err(|error| TardisError::bad_request(&format!("[SG.server] Bind address error: {error}"), ""))?;
            let server = Server::builder(TlsAcceptor::new(tls_cfg, incoming)).serve(make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(sg_process)) }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            serv_futures.push(server.boxed());
        } else {
            let server = Server::bind(&addr).serve(make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(sg_process)) }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            serv_futures.push(server.boxed());
        }
        log::info!("[SG.server] Listening on http://{} ", addr);
    }

    let mut shutdown = SHUTDOWN_TX.lock().await;
    shutdown.insert(gateway_conf.name, shutdown_tx);

    join_all(serv_futures).await;
    Ok(())
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    let mut shutdown = SHUTDOWN_TX.lock().await;
    if let Some(shutdown_tx) = shutdown.remove(gateway_name) {
        shutdown_tx.send(()).map_err(|_| TardisError::bad_request("[SG.server] Shutdown failed", ""))?;
    }
    Ok(())
}

struct TlsAcceptor {
    config: Arc<ServerConfig>,
    incoming: AddrIncoming,
}

impl TlsAcceptor {
    pub fn new(config: Arc<ServerConfig>, incoming: AddrIncoming) -> TlsAcceptor {
        TlsAcceptor { config, incoming }
    }
}

impl Accept for TlsAcceptor {
    type Conn = TlsStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let pin = self.get_mut();
        match ready!(Pin::new(&mut pin.incoming).poll_accept(cx)) {
            Some(Ok(sock)) => Poll::Ready(Some(Ok(TlsStream::new(sock, pin.config.clone())))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}

enum State {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

struct TlsStream {
    state: State,
}

impl TlsStream {
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> TlsStream {
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        TlsStream {
            state: State::Handshaking(accept),
        }
    }
}

impl AsyncRead for TlsStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

async fn sg_process(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        // Help route.
        (&Method::GET, "/") => {
            *response.body_mut() = Body::from("Try POST /echo\n");
        }
        // Echo service route.
        (&Method::POST, "/echo") => {
            *response.body_mut() = req.into_body();
        }
        // Catch-all 404.
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };

    // let mut not_found = Response::default();
    //         *not_found.status_mut() = StatusCode::NOT_FOUND;
    //         Ok(not_found)

    Ok(response)
}
