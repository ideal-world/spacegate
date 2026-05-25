use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use futures_util::future::BoxFuture;
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::TokioIo;
use opentelemetry::trace::TraceContextExt;
use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::{
    extension::{BackendHost, EnterTime, PeerAddr, Reflect, RouteName},
    observability::{
        access_log_fields, client_ip, content_length, header_value, http_protocol_version, record_http_server_active_request, record_http_server_metrics_with_labels,
        telemetry_json, HttpMetricLabels, TelemetryContext,
    },
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
    gateway_name: Arc<str>,
    connection_builder: ConnectionBuilder,
}

impl Http {
    pub fn new(service: ArcHyperService) -> Self {
        Self::with_gateway_name(service, Arc::<str>::from("unknown"))
    }

    pub fn with_gateway_name(service: ArcHyperService, gateway_name: Arc<str>) -> Self {
        Self {
            inner_service: service,
            gateway_name,
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
        let service = HyperServiceAdapter::with_gateway_name(self.inner_service.clone(), peer, self.gateway_name.clone());
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
    gateway_name: Arc<str>,
    tls_config: Arc<rustls::ServerConfig>,
    connection_builder: ConnectionBuilder,
}

impl Https {
    pub fn new(service: ArcHyperService, tls_config: rustls::ServerConfig) -> Self {
        Self::with_gateway_name(service, tls_config, Arc::<str>::from("unknown"))
    }

    pub fn with_gateway_name(service: ArcHyperService, tls_config: rustls::ServerConfig, gateway_name: Arc<str>) -> Self {
        Self {
            inner_service: service,
            gateway_name,
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
        let service = HyperServiceAdapter::with_gateway_name(self.inner_service.clone(), peer, self.gateway_name.clone());
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
    gateway_name: Arc<str>,
}

impl<S> HyperServiceAdapter<S>
where
    S: hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    pub fn new(service: S, peer: SocketAddr) -> Self {
        Self::with_gateway_name(service, peer, Arc::<str>::from("unknown"))
    }

    pub fn with_gateway_name(service: S, peer: SocketAddr, gateway_name: Arc<str>) -> Self {
        Self { service, peer, gateway_name }
    }

    pub fn gateway_name(&self) -> &str {
        self.gateway_name.as_ref()
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
        let method = req.method().clone();
        let method_label = method.as_str().to_string();
        let path = req.uri().path().to_string();
        let host = req.uri().host().map(str::to_string).or_else(|| req.headers().get(hyper::header::HOST).and_then(|v| v.to_str().ok()).map(str::to_string)).unwrap_or_default();
        let protocol = format!("{:?}", req.version());
        let protocol_version_label = http_protocol_version(req.version());
        let request_id = req.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
        let x_forwarded_for = header_value(req.headers(), "x-forwarded-for");
        let user_agent = header_value(req.headers(), "user-agent");
        let client_ip_label = client_ip(req.headers(), self.peer);
        let request_body_size = content_length(req.headers());
        let peer_addr_label = self.peer.to_string();
        let span = tracing::info_span!(
            "http.server.request",
            http.method = %method,
            http.path = %path,
            http.host = %host,
            http.protocol = %protocol,
            http.status_code = tracing::field::Empty,
            request_id = %request_id,
            peer_addr = %self.peer,
            duration_ms = tracing::field::Empty
        );
        let gateway_label = self.gateway_name.to_string();
        let telemetry_context = TelemetryContext::default();
        let active_request_labels = HttpMetricLabels {
            gateway: gateway_label.clone(),
            method: method_label.clone(),
            status_code: "active".to_string(),
            protocol_name: "http".to_string(),
            protocol_version: protocol_version_label.clone(),
            request_body_size,
            response_body_size: None,
        };
        record_http_server_active_request(active_request_labels.clone(), 1);
        let mut reflect = Reflect::default();
        // let method = req.method().clone();
        reflect.insert(enter_time);
        req.extensions_mut().insert(reflect);
        req.extensions_mut().insert(PeerAddr(self.peer));
        req.extensions_mut().insert(enter_time);
        req.extensions_mut().insert(telemetry_context.clone());
        let span_for_recording = span.clone();
        Box::pin(
            async move {
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
                let latency = enter_time.elapsed();
                span_for_recording.record("http.status_code", status.as_u16());
                span_for_recording.record("duration_ms", latency.as_millis() as u64);
                let response_body_size = content_length(resp.headers());
                let access_request_id = resp.headers().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_string).unwrap_or(request_id);
                tracing::trace!(latency = ?latency, "request finished");
                let authority = host.clone();
                let route_name = resp.extensions().get::<RouteName>().map(|route| route.to_string()).unwrap_or_default();
                let upstream_host = resp.extensions().get::<BackendHost>().map(|host| host.to_string()).unwrap_or_default();
                let trace_id = span_for_recording.context().span().span_context().trace_id().to_string();
                record_http_server_metrics_with_labels(
                    HttpMetricLabels {
                        gateway: gateway_label.clone(),
                        method: method_label.clone(),
                        status_code: status.as_u16().to_string(),
                        protocol_name: "http".to_string(),
                        protocol_version: protocol_version_label.clone(),
                        request_body_size,
                        response_body_size,
                    },
                    latency,
                    status.is_server_error() || status.is_client_error(),
                );
                let access_log = access_log_fields(
                    gateway_label,
                    method_label,
                    path,
                    host,
                    client_ip_label,
                    x_forwarded_for,
                    user_agent,
                    authority,
                    peer_addr_label.clone(),
                    route_name,
                    upstream_host,
                    trace_id,
                    protocol_version_label,
                    status,
                    access_request_id,
                    peer_addr_label,
                    latency,
                    request_body_size,
                    response_body_size,
                    telemetry_context.snapshot(),
                );
                let telemetry = telemetry_json(&access_log.telemetry);
                tracing::info!(
                    event = "http_access",
                    gateway = %access_log.gateway,
                    method = %access_log.method,
                    path = %access_log.path,
                    host = %access_log.host,
                    authority = %access_log.authority,
                    client_ip = %access_log.client_ip,
                    x_forwarded_for = %access_log.x_forwarded_for,
                    user_agent = %access_log.user_agent,
                    downstream_remote_address = %access_log.downstream_remote_address,
                    route_name = %access_log.route_name,
                    upstream_host = %access_log.upstream_host,
                    trace_id = %access_log.trace_id,
                    protocol_name = %access_log.protocol_name,
                    protocol_version = %access_log.protocol_version,
                    status_code = access_log.status_code,
                    request_id = %access_log.request_id,
                    peer_addr = %access_log.peer_addr,
                    duration_ms = access_log.duration_ms,
                    bytes_received = ?access_log.request_body_size,
                    bytes_sent = ?access_log.response_body_size,
                    request_body_size = ?access_log.request_body_size,
                    response_body_size = ?access_log.response_body_size,
                    telemetry = %telemetry,
                    "http access log"
                );
                record_http_server_active_request(active_request_labels, -1);
                Ok(resp)
            }
            .instrument(span),
        )
    }
}

impl ArcHyperService {
    pub fn http(self) -> Http {
        Http::new(self)
    }
    pub fn https(self, tls_config: rustls::ServerConfig) -> Https {
        Https::new(self, tls_config)
    }
    pub fn http_with_gateway_name(self, gateway_name: Arc<str>) -> Http {
        Http::with_gateway_name(self, gateway_name)
    }
    pub fn https_with_gateway_name(self, tls_config: rustls::ServerConfig, gateway_name: Arc<str>) -> Https {
        Https::with_gateway_name(self, tls_config, gateway_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyper_service_adapter_keeps_gateway_name_from_listener() {
        let service = hyper::service::service_fn(|_req: Request<SgBody>| async { Ok::<_, Infallible>(Response::new(SgBody::empty())) });
        let peer = "127.0.0.1:12345".parse().expect("peer");

        let adapter = HyperServiceAdapter::with_gateway_name(service, peer, Arc::<str>::from("gw-a"));

        assert_eq!(adapter.gateway_name(), "gw-a");
    }
}
