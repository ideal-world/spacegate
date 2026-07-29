use std::{
    convert::Infallible,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::future::BoxFuture;
use hyper::{
    body::{Body, Bytes, Frame, Incoming},
    Request, Response, StatusCode,
};
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
        telemetry_json, AccessLogContext, HttpMetricLabels, TelemetryContext,
    },
    ArcHyperService, BoxResult, SgBody,
};

/// 在响应 body 完成或客户端断开时输出 access log 的收尾状态。
struct AccessLogFinalizer {
    finalized: bool,
    enter_time: EnterTime,
    span: tracing::Span,
    telemetry_context: TelemetryContext,
    gateway: String,
    method: String,
    path: String,
    host: String,
    client_ip: String,
    x_forwarded_for: String,
    user_agent: String,
    authority: String,
    downstream_remote_address: String,
    route_name: String,
    upstream_host: String,
    protocol_version: String,
    status: StatusCode,
    request_id: String,
    peer_addr: String,
    request_body_size: Option<u64>,
    response_body_size: Option<u64>,
    active_request_labels: HttpMetricLabels,
}

impl AccessLogFinalizer {
    /// 只执行一次日志、指标和活动请求收尾，保证 EOF 与 Drop 不会重复记录。
    fn finish(&mut self) {
        if self.finalized {
            return;
        }
        self.finalized = true;
        let latency = self.enter_time.elapsed();
        self.span.record("http.status_code", self.status.as_u16());
        self.span.record("duration_ms", latency.as_millis() as u64);
        let span_context = self.span.context();
        let span = span_context.span();
        let otel_span_context = span.span_context();
        let trace_id = otel_span_context.is_valid().then(|| otel_span_context.trace_id().to_string()).unwrap_or_default();
        record_http_server_metrics_with_labels(
            HttpMetricLabels {
                gateway: self.gateway.clone(),
                method: self.method.clone(),
                status_code: self.status.as_u16().to_string(),
                protocol_name: "http".to_string(),
                protocol_version: self.protocol_version.clone(),
                request_body_size: self.request_body_size,
                response_body_size: self.response_body_size,
            },
            latency,
            self.status.is_server_error() || self.status.is_client_error(),
        );
        let access_log = access_log_fields(
            self.gateway.clone(),
            self.method.clone(),
            self.path.clone(),
            self.host.clone(),
            self.client_ip.clone(),
            self.x_forwarded_for.clone(),
            self.user_agent.clone(),
            self.authority.clone(),
            self.downstream_remote_address.clone(),
            self.route_name.clone(),
            self.upstream_host.clone(),
            trace_id,
            self.protocol_version.clone(),
            self.status,
            self.request_id.clone(),
            self.peer_addr.clone(),
            latency,
            self.request_body_size,
            self.response_body_size,
            self.telemetry_context.snapshot(),
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
        record_http_server_active_request(self.active_request_labels.clone(), -1);
    }
}

/// 保持响应数据原样透传，并将 access-log 收尾推迟到 body 生命周期结束。
struct AccessLogBody {
    inner: SgBody,
    finalizer: AccessLogFinalizer,
}

impl Body for AccessLogBody {
    type Data = Bytes;
    type Error = crate::BoxError;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        let poll = Pin::new(&mut this.inner).poll_frame(cx);
        if matches!(poll, Poll::Ready(None) | Poll::Ready(Some(Err(_)))) {
            this.finalizer.finish();
        }
        poll
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for AccessLogBody {
    fn drop(&mut self) {
        self.finalizer.finish();
    }
}

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
        let access_log_context = AccessLogContext::default();
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
        req.extensions_mut().insert(access_log_context.clone());
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
                let response_body_size = content_length(resp.headers());
                let access_request_id = resp.headers().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_string).unwrap_or(request_id);
                let authority = host.clone();
                let route_name = resp.extensions().get::<RouteName>().map(|route| route.to_string()).unwrap_or_else(|| access_log_context.route_name());
                let upstream_host = resp.extensions().get::<BackendHost>().map(|host| host.to_string()).unwrap_or_else(|| access_log_context.upstream_host());
                let finalizer = AccessLogFinalizer {
                    finalized: false,
                    enter_time,
                    span: span_for_recording,
                    telemetry_context,
                    gateway: gateway_label,
                    method: method_label,
                    path,
                    host,
                    client_ip: client_ip_label,
                    x_forwarded_for,
                    user_agent,
                    authority,
                    downstream_remote_address: peer_addr_label.clone(),
                    route_name,
                    upstream_host,
                    protocol_version: protocol_version_label,
                    status,
                    request_id: access_request_id,
                    peer_addr: peer_addr_label,
                    request_body_size,
                    response_body_size,
                    active_request_labels,
                };
                let (parts, body) = resp.into_parts();
                Ok(Response::from_parts(parts, SgBody::new(AccessLogBody { inner: body, finalizer })))
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

    fn test_access_log_finalizer() -> AccessLogFinalizer {
        AccessLogFinalizer {
            finalized: false,
            enter_time: EnterTime::new(),
            span: tracing::info_span!("access_log_body_test"),
            telemetry_context: TelemetryContext::default(),
            gateway: "test".to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
            host: "test".to_string(),
            client_ip: "127.0.0.1".to_string(),
            x_forwarded_for: String::new(),
            user_agent: String::new(),
            authority: "test".to_string(),
            downstream_remote_address: "127.0.0.1:12345".to_string(),
            route_name: String::new(),
            upstream_host: String::new(),
            protocol_version: "1.1".to_string(),
            status: StatusCode::OK,
            request_id: "request-1".to_string(),
            peer_addr: "127.0.0.1:12345".to_string(),
            request_body_size: None,
            response_body_size: Some(1),
            active_request_labels: HttpMetricLabels {
                gateway: "test".to_string(),
                method: "GET".to_string(),
                status_code: "active".to_string(),
                protocol_name: "http".to_string(),
                protocol_version: "1.1".to_string(),
                request_body_size: None,
                response_body_size: None,
            },
        }
    }

    #[test]
    fn hyper_service_adapter_keeps_gateway_name_from_listener() {
        let service = hyper::service::service_fn(|_req: Request<SgBody>| async { Ok::<_, Infallible>(Response::new(SgBody::empty())) });
        let peer = "127.0.0.1:12345".parse().expect("peer");

        let adapter = HyperServiceAdapter::with_gateway_name(service, peer, Arc::<str>::from("gw-a"));

        assert_eq!(adapter.gateway_name(), "gw-a");
    }

    #[test]
    fn access_log_body_finishes_only_after_response_eof() {
        let mut body = AccessLogBody {
            inner: SgBody::full("x"),
            finalizer: test_access_log_finalizer(),
        };
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        let Poll::Ready(Some(Ok(frame))) = Pin::new(&mut body).poll_frame(&mut context) else {
            panic!("first response frame must be ready");
        };
        assert_eq!(frame.into_data().expect("response data"), b"x"[..]);
        assert!(!body.finalizer.finalized);

        assert!(matches!(Pin::new(&mut body).poll_frame(&mut context), Poll::Ready(None)));
        assert!(body.finalizer.finalized);
    }
}
