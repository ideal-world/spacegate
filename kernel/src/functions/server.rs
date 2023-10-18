use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
};

use core::task::{Context, Poll};
use http::{HeaderValue, Request, Response, StatusCode};
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use hyper::{server::accept::Accept, Body};
use kernel_dto::dto::gateway_dto::{SgGateway, SgProtocol, SgTlsMode};

use lazy_static::lazy_static;
use rustls::{PrivateKey, ServerConfig};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use std::{io, sync};
use tardis::basic::tracing::TardisTracing;
use tardis::tokio::time::timeout;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log::{self},
    tokio::{self, sync::watch::Sender, task::JoinHandle},
    TardisFuns,
};
use tardis::{
    futures_util::{ready, FutureExt},
    tokio::sync::Mutex,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::http_route;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
    static ref START_JOIN_HANDLE: Arc<Mutex<HashMap<String, JoinHandle<()>>>> = <_>::default();
}

pub async fn init(gateway_conf: &SgGateway) -> TardisResult<Vec<SgServerInst>> {
    if gateway_conf.listeners.is_empty() {
        return Err(TardisError::bad_request("[SG.Server] Missing Listeners", ""));
    }
    if gateway_conf.listeners.iter().any(|l| l.protocol != SgProtocol::Http && l.protocol != SgProtocol::Https && l.protocol != SgProtocol::Ws) {
        return Err(TardisError::bad_request("[SG.Server] Non-Http(s) protocols are not supported yet", ""));
    }
    if let Some(log_level) = gateway_conf.parameters.log_level.clone() {
        log::debug!("[SG.Server] change log level to {log_level}");
        TardisTracing::update_log_level_by_domain_code(crate::constants::DOMAIN_CODE, &log_level)?;
    }
    let (shutdown_tx, _) = tokio::sync::watch::channel(());

    let gateway_name = Arc::new(gateway_conf.name.to_string());
    let mut server_insts: Vec<SgServerInst> = Vec::new();
    for listener in &gateway_conf.listeners {
        let ip = listener.ip.as_deref().unwrap_or("0.0.0.0");
        let addr = if ip.contains('.') {
            let ip: Ipv4Addr = ip.parse().map_err(|_| TardisError::bad_request(&format!("[SG.Server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V4(ip), listener.port)
        } else {
            let ip: Ipv6Addr = ip.parse().map_err(|_| TardisError::bad_request(&format!("[SG.Server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V6(ip), listener.port)
        };

        let mut shutdown_rx = shutdown_tx.subscribe();

        let gateway_name = gateway_name.clone();
        let protocol = listener.protocol.to_string();
        if let Some(tls) = &listener.tls {
            log::debug!("[SG.Server] Tls is init...mode:{:?}", tls.mode);
            if SgTlsMode::Terminate == tls.mode {
                let tls_cfg = {
                    let certs = rustls_pemfile::certs(&mut tls.cert.as_bytes())
                        .map_err(|error| TardisError::bad_request(&format!("[SG.Server] Tls certificates not legal: {error}"), ""))?;
                    let certs = certs.into_iter().map(rustls::Certificate).collect::<Vec<_>>();
                    let key = rustls_pemfile::read_all(&mut tls.key.as_bytes())
                        .map_err(|error| TardisError::bad_request(&format!("[SG.Server] Tls private keys not legal: {error}"), ""))?;
                    if key.is_empty() {
                        return Err(TardisError::bad_request("[SG.Server] not found Tls private key", ""));
                    }
                    let mut selected_key = None;
                    for k in key {
                        selected_key = match k {
                            rustls_pemfile::Item::X509Certificate(_) => continue,
                            rustls_pemfile::Item::RSAKey(k) => Some(k),
                            rustls_pemfile::Item::PKCS8Key(k) => Some(k),
                            rustls_pemfile::Item::ECKey(k) => Some(k),
                            _ => continue,
                        };
                        if selected_key.is_some() {
                            break;
                        }
                    }
                    if let Some(selected_key) = selected_key {
                        let key = PrivateKey(selected_key);
                        let mut cfg = rustls::ServerConfig::builder()
                            .with_safe_defaults()
                            .with_no_client_auth()
                            .with_single_cert(certs, key)
                            .map_err(|error| TardisError::bad_request(&format!("[SG.Server] Tls not legal: {error}"), ""))?;
                        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                        sync::Arc::new(cfg)
                    } else {
                        return Err(TardisError::not_implemented("[SG.Server] Tls encoding not supported ", ""));
                    }
                };

                let incoming = AddrIncoming::bind(&addr).map_err(|error| TardisError::bad_request(&format!("[SG.Server] Bind address error: {error}"), ""))?;
                let server = Server::builder(TlsAcceptor::new(tls_cfg, incoming)).serve(make_service_fn(move |client: &TlsStream| {
                    let protocol = Arc::new(protocol.clone());
                    let remote_and_local_addr = match &client.state {
                        State::Handshaking(addr) => (
                            addr.get_ref().expect("[SG.server.init] can't get addr").remote_addr(),
                            addr.get_ref().expect("[SG.server.init] can't get addr").local_addr(),
                        ),
                        State::Streaming(addr) => (addr.get_ref().0.remote_addr(), addr.get_ref().0.local_addr()),
                    };
                    let gateway_name = gateway_name.clone();
                    async move { Ok::<_, Infallible>(service_fn(move |req| process(gateway_name.clone(), protocol.clone(), remote_and_local_addr, req))) }
                }));
                let server = server.with_graceful_shutdown(async move {
                    shutdown_rx.changed().await.ok();
                });
                server_insts.push(SgServerInst { addr, server: server.boxed() });
            } else {
                let server = Server::bind(&addr).serve(make_service_fn(move |client: &AddrStream| {
                    let protocol = Arc::new(protocol.clone());
                    let remote_addr = client.remote_addr();
                    let local_addr = client.local_addr();
                    let gateway_name = gateway_name.clone();
                    async move { Ok::<_, Infallible>(service_fn(move |req| process(gateway_name.clone(), protocol.clone(), (remote_addr, local_addr), req))) }
                }));
                let server = server.with_graceful_shutdown(async move {
                    shutdown_rx.changed().await.ok();
                });
                server_insts.push(SgServerInst { addr, server: server.boxed() });
            }
        } else {
            let server = Server::bind(&addr).serve(make_service_fn(move |client: &AddrStream| {
                let protocol = Arc::new(protocol.clone());
                let remote_and_local_addr = (client.remote_addr(), client.local_addr());
                let gateway_name = gateway_name.clone();
                async move { Ok::<_, Infallible>(service_fn(move |req| process(gateway_name.clone(), protocol.clone(), remote_and_local_addr, req))) }
            }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            server_insts.push(SgServerInst { addr, server: server.boxed() });
        }
    }

    let mut shutdown = SHUTDOWN_TX.lock().await;
    shutdown.insert(gateway_name.to_string(), shutdown_tx);

    Ok(server_insts)
}

async fn process(
    gateway_name: Arc<String>,
    req_scheme: Arc<String>,
    (remote_addr, local_addr): (SocketAddr, SocketAddr),
    request: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    let method = request.method().to_string().clone();
    let uri = request.uri().to_string().clone();
    let response = http_route::process(gateway_name, req_scheme.as_str(), (remote_addr, local_addr), request).await;
    let result = match response {
        Ok(result) => Ok(result),
        Err(error) => into_http_error(error),
    };
    match &result {
        Ok(resp) => {
            if log::level_enabled!(log::Level::TRACE) {
                log::trace!(
                    "[SG.server] Response: code {} => {} {} headers {:?} body {:?}",
                    resp.status(),
                    method,
                    uri,
                    resp.headers(),
                    resp.body(),
                );
            } else if log::level_enabled!(log::Level::DEBUG) {
                log::debug!("[SG.server] Response: code {} => {} {} headers {:?} ", resp.status(), method, uri, resp.headers(),);
            } else if !resp.status().is_success() {
                log::info!("[SG.server] Response: code {} => {} {}", resp.status(), method, uri);
            }
        }
        Err(e) => log::warn!("[SG.server] Response: error {} => {} {}", e.message(), method, uri),
    }
    result
}

fn into_http_error(error: TardisError) -> Result<Response<Body>, hyper::Error> {
    let status_code = match error.code.parse::<u16>() {
        Ok(code) => match StatusCode::from_u16(code) {
            Ok(status_code) => status_code,
            Err(_) => {
                if (200..400).contains(&code) {
                    StatusCode::OK
                } else if (400..500).contains(&code) {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }
        },
        Err(_) => {
            if error.code.starts_with('2') || error.code.starts_with('3') {
                StatusCode::OK
            } else if error.code.starts_with('4') {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    };
    let mut response = Response::new(Body::from(
        TardisFuns::json
            .json_to_string(json!({
                "code": error.code,
                "msg": error.message,
            }))
            .expect("TardisFuns.json_to_string error"),
    ));
    *response.status_mut() = status_code;
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
    Ok(response)
}

pub async fn startup(gateway_name: &str, servers: Vec<SgServerInst>) -> TardisResult<()> {
    for server in &servers {
        log::info!("[SG.server] Listening on http://{} ", server.addr);
    }
    let servers = servers.into_iter().map(|s| s.server).collect::<Vec<_>>();
    let handle = tokio::spawn(async move {
        join_all(servers).await;
    });
    let mut handle_guard = START_JOIN_HANDLE.lock().await;
    handle_guard.insert(gateway_name.to_string(), handle);
    Ok(())
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    let mut shutdown = SHUTDOWN_TX.lock().await;
    if let Some(shutdown_tx) = shutdown.remove(gateway_name) {
        shutdown_tx.send(()).map_err(|_| TardisError::bad_request("[SG.Server] Shutdown failed", ""))?;
    }
    let mut handle_guard: tokio::sync::MutexGuard<HashMap<String, JoinHandle<()>>> = START_JOIN_HANDLE.lock().await;
    if let Some(handle) = handle_guard.remove(gateway_name) {
        match timeout(Duration::from_millis(1000), handle).await {
            Ok(response) => response.map_err(|e| TardisError::bad_gateway(&format!("[SG.Server] Wait shutdown failed:{e}"), "")),
            Err(e) => {
                log::warn!("[SG.Server] Wait shutdown timeout:{e}");
                Ok(())
            }
        }?;
        log::info!("[SG.Server] Gateway shutdown");
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

pub struct SgServerInst {
    pub addr: SocketAddr,
    pub server: Pin<Box<dyn std::future::Future<Output = Result<(), hyper::Error>> + std::marker::Send>>,
}
