use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
};

use crate::config::gateway_dto::{SgGateway, SgProtocol};
use core::task::{Context, Poll};
use http::{HeaderValue, Request, Response, StatusCode};
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use hyper::{server::accept::Accept, Body};
use lazy_static::lazy_static;
use rustls::{PrivateKey, ServerConfig};
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::vec::Vec;
use std::{io, sync};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log::{self, info, LevelFilter},
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
    static ref START_JOIN_HANDLE: Arc<Mutex<Option<JoinHandle<()>>>> = <_>::default();
}

pub async fn init(gateway_conf: &SgGateway) -> TardisResult<Vec<SgServerInst>> {
    if gateway_conf.listeners.is_empty() {
        return Err(TardisError::bad_request("[SG.Server] Missing Listeners", ""));
    }
    if gateway_conf.listeners.iter().any(|l| l.protocol != SgProtocol::Http && l.protocol != SgProtocol::Https) {
        return Err(TardisError::bad_request("[SG.Server] Non-Http(s) protocols are not supported yet", ""));
    }
    if let Some(log_level) = gateway_conf.parameters.log_level.clone() {
        log::set_max_level(LevelFilter::from_str(&log_level).unwrap_or(log::max_level()))
    }
    log::info!("[SG.Server] Gateway use log level:{}", log::max_level());
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
        if let Some(tls) = &listener.tls {
            log::debug!("[SG.Server] Tls is init...");
            let tls_cfg = {
                let certs = rustls_pemfile::certs(&mut tls_base64_decode(&tls.cert)?.as_bytes())
                    .map_err(|error| TardisError::bad_request(&format!("[SG.Server] Tls certificates not legal: {error}"), ""))?;
                let certs = certs.into_iter().map(rustls::Certificate).collect::<Vec<_>>();
                let key = rustls_pemfile::read_all(&mut tls_base64_decode(&tls.key)?.as_bytes())
                    .map_err(|error| TardisError::bad_request(&format!("[SG.Server] Tls private keys not legal: {error}"), ""))?;
                if key.is_empty() {
                    return Err(TardisError::bad_request(&format!("[SG.Server] not found Tls private key"), ""));
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
                    return Err(TardisError::not_implemented(&format!("[SG.Server] Tls encoding not supported "), ""));
                }
            };
            let incoming = AddrIncoming::bind(&addr).map_err(|error| TardisError::bad_request(&format!("[SG.Server] Bind address error: {error}"), ""))?;
            let server = Server::builder(TlsAcceptor::new(tls_cfg, incoming)).serve(make_service_fn(move |client: &TlsStream| {
                let remote_addr = match &client.state {
                    State::Handshaking(addr) => addr.get_ref().unwrap().remote_addr(),
                    State::Streaming(addr) => addr.get_ref().0.remote_addr(),
                };
                let gateway_name = gateway_name.clone();
                async move { Ok::<_, Infallible>(service_fn(move |req| process(gateway_name.clone(), "https", remote_addr, req))) }
            }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            server_insts.push(SgServerInst { addr, server: server.boxed() });
        } else {
            let server = Server::bind(&addr).serve(make_service_fn(move |client: &AddrStream| {
                let remote_addr = client.remote_addr();
                let gateway_name = gateway_name.clone();
                async move { Ok::<_, Infallible>(service_fn(move |req| process(gateway_name.clone(), "http", remote_addr, req))) }
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

fn tls_base64_decode(mut key: &str) -> TardisResult<String> {
    if key.starts_with('"') {
        key = &key[1..];
    }
    if key.ends_with('"') {
        key = &key[..key.len() - 1];
    }
    if let Ok(key) = TardisFuns::crypto.base64.decode(key) {
        Ok(key)
    } else {
        Ok(key.to_string())
    }
}

async fn process(gateway_name: Arc<String>, req_scheme: &str, remote_addr: SocketAddr, request: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let response = http_route::process(gateway_name, req_scheme, remote_addr, request).await;
    match response {
        Ok(result) => Ok(result),
        Err(error) => into_http_error(error),
    }
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
            .unwrap(),
    ));
    *response.status_mut() = status_code;
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
    Ok(response)
}

pub async fn startup(servers: Vec<SgServerInst>) -> TardisResult<()> {
    for server in &servers {
        log::info!("[SG.server] Listening on http://{} ", server.addr);
    }
    let servers = servers.into_iter().map(|s| s.server).collect::<Vec<_>>();
    let handle = tokio::spawn(async move {
        join_all(servers).await;
    });
    let mut handle_guard = START_JOIN_HANDLE.lock().await;
    *handle_guard = Some(handle);
    Ok(())
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    let mut shutdown = SHUTDOWN_TX.lock().await;
    if let Some(shutdown_tx) = shutdown.remove(gateway_name) {
        shutdown_tx.send(()).map_err(|_| TardisError::bad_request("[SG.Server] Shutdown failed", ""))?;
    }
    let mut handle_guard: tokio::sync::MutexGuard<Option<JoinHandle<()>>> = START_JOIN_HANDLE.lock().await;
    if handle_guard.is_some() {
        let mut swap_handle: Option<JoinHandle<()>> = None;
        std::mem::swap(&mut swap_handle, &mut *handle_guard);
        swap_handle.unwrap().await.map_err(|e| TardisError::bad_request(&format!("[SG.Server] Wait shutdown failed:{e}"), ""))?;
        log::info!("[SG.Server] Gateway shutdown");
    } else {
        log::warn!("[SG.Server] Can't found server join handle , you may have called shutdown before start");
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
