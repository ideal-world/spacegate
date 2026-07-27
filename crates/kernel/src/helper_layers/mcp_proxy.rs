#[cfg(test)]
mod tests {
    use super::McpProxyLayer;
    use crate::{
        extension::McpProxyMeta,
        helper_layers::function::FnLayer,
        observability::TelemetryContext,
        SgBody,
    };
    use hyper::{Request, Response, StatusCode};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tower_layer::Layer;

    /// Verifies that an outer authentication plugin can reject an MCP request before transport handling reaches upstream.
    #[tokio::test]
    async fn mcp_auth_layer_can_short_circuit_before_transport_and_upstream() {
        let upstream_called = Arc::new(AtomicBool::new(false));
        let upstream_called_by_service = upstream_called.clone();
        let upstream = hyper::service::service_fn(move |_request: Request<SgBody>| {
            upstream_called_by_service.store(true, Ordering::SeqCst);
            async move { Ok::<_, std::convert::Infallible>(Response::new(SgBody::empty())) }
        });
        let transport = McpProxyLayer::streamable_http().layer(upstream);
        let auth = FnLayer::new_closure(|_request, _inner| async move {
            Response::builder().status(StatusCode::UNAUTHORIZED).body(SgBody::empty()).expect("authentication response")
        });
        let service = auth.layer(transport);
        let request = Request::builder().method("GET").uri("/mcp").header("accept", "text/event-stream").body(SgBody::empty()).expect("request");

        let response = hyper::service::Service::call(&service, request).await.expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!upstream_called.load(Ordering::SeqCst));
    }

    /// Verifies that an outer route plugin can replace a finite JSON response produced through the MCP transport layer.
    #[tokio::test]
    async fn mcp_outer_plugin_can_replace_json_body() {
        let upstream = hyper::service::service_fn(|_request: Request<SgBody>| async move {
            Ok::<_, std::convert::Infallible>(
                Response::builder().header("content-type", "application/json").body(SgBody::full(r#"{"result":"original"}"#)).expect("upstream response"),
            )
        });
        let transport = McpProxyLayer::streamable_http().layer(upstream);
        let response_rewriter = FnLayer::new_closure(|request, inner| async move {
            let response = inner.call(request).await;
            response.map(|_body| SgBody::full(r#"{"result":"rewritten"}"#))
        });
        let service = response_rewriter.layer(transport);
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(SgBody::full(r#"{"jsonrpc":"2.0"}"#))
            .expect("request");

        let response = hyper::service::Service::call(&service, request).await.expect("response");
        let body = response.into_body().dump().await.expect("response body");

        assert_eq!(body.get_dumped().expect("dumped response body"), r#"{"result":"rewritten"}"#);
    }

    /// Verifies that a Streamable HTTP session termination DELETE reaches the upstream unchanged.
    #[tokio::test]
    async fn streamable_http_delete_forwards_session_header() {
        let upstream = hyper::service::service_fn(|request: Request<SgBody>| async move {
            assert_eq!(request.method(), hyper::Method::DELETE);
            assert_eq!(request.headers().get("Mcp-Session-Id").and_then(|value| value.to_str().ok()), Some("session-1"));
            Ok::<_, std::convert::Infallible>(Response::new(SgBody::empty()))
        });
        let service = McpProxyLayer::streamable_http().layer(upstream);
        let request = Request::builder().method("DELETE").uri("/mcp").header("Mcp-Session-Id", "session-1").body(SgBody::empty()).expect("request");

        let response = hyper::service::Service::call(&service, request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Verifies that Legacy SSE message POST accepts JSON without an unnecessary Accept requirement.
    #[tokio::test]
    async fn legacy_sse_post_does_not_require_accept_header() {
        let upstream = hyper::service::service_fn(|_request: Request<SgBody>| async move { Ok::<_, std::convert::Infallible>(Response::new(SgBody::empty())) });
        let service = McpProxyLayer::legacy_sse().layer(upstream);
        let request = Request::builder().method("POST").uri("/message").header("content-type", "application/json").body(SgBody::full("{}")).expect("request");

        let response = hyper::service::Service::call(&service, request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn streamable_http_request_records_safe_mcp_telemetry() {
        let telemetry = TelemetryContext::default();
        let mut request =
            Request::builder().method("GET").uri("/mcp").header("accept", "text/event-stream").header("mcp-session-id", "session-123").body(SgBody::empty()).expect("request");
        request.extensions_mut().insert(telemetry.clone());
        let service = hyper::service::service_fn(|request: Request<SgBody>| async move {
            let meta = request.extensions().get::<McpProxyMeta>().expect("MCP proxy metadata");
            assert_eq!(meta.transport, "streamable_http");
            assert!(meta.session_id_present);
            Ok::<_, std::convert::Infallible>(Response::new(SgBody::empty()))
        });
        let mut service = McpProxyLayer::streamable_http().layer(service);

        let response = hyper::service::Service::call(&mut service, request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(telemetry.snapshot().get("mcp.transport").map(String::as_str), Some("streamable_http"));
        assert_eq!(telemetry.snapshot().get("mcp.session_id_present").map(String::as_str), Some("true"));
    }

    #[tokio::test]
    async fn streamable_http_rejects_non_json_post_without_calling_upstream() {
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "text/plain")
            .body(SgBody::empty())
            .expect("request");
        let service = hyper::service::service_fn(|_request: Request<SgBody>| async move { panic!("invalid MCP request must not reach upstream") });
        let mut service = McpProxyLayer::streamable_http().layer(service);

        let response = hyper::service::Service::call(&mut service, request).await.expect("response");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
// MCP transport guard for routes that transparently proxy external MCP servers.
// The layer validates transport-level HTTP semantics only. It deliberately never reads
// JSON-RPC bodies, so SSE and streaming POST responses remain transparent.

use std::convert::Infallible;

use futures_util::future::BoxFuture;
use hyper::{header, Method, Request, Response, StatusCode};
use tower_layer::Layer;

use crate::{extension::McpProxyMeta, observability::TelemetryContext, SgBody};

/// MCP remote transport selected by a compiled MCPRoute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// The current MCP Streamable HTTP transport.
    StreamableHttp,
    /// The legacy HTTP plus SSE compatibility transport.
    LegacySse,
}

impl McpTransport {
    /// Stable telemetry value for the configured transport.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StreamableHttp => "streamable_http",
            Self::LegacySse => "legacy_sse",
        }
    }
}

/// Adds MCP request validation, metadata and safe observability fields to a route rule.
#[derive(Debug, Clone, Copy)]
pub struct McpProxyLayer {
    transport: McpTransport,
}

impl McpProxyLayer {
    /// Creates a Streamable HTTP MCP proxy layer.
    pub const fn streamable_http() -> Self {
        Self {
            transport: McpTransport::StreamableHttp,
        }
    }

    /// Creates a legacy SSE MCP proxy layer.
    pub const fn legacy_sse() -> Self {
        Self {
            transport: McpTransport::LegacySse,
        }
    }

    /// Creates a layer for the explicitly configured MCP transport.
    pub const fn new(transport: McpTransport) -> Self {
        Self { transport }
    }
}

impl<S> Layer<S> for McpProxyLayer {
    type Service = McpProxy<S>;

    fn layer(&self, inner: S) -> Self::Service {
        McpProxy { inner, transport: self.transport }
    }
}

/// Service created by [`McpProxyLayer`].
#[derive(Debug, Clone)]
pub struct McpProxy<S> {
    inner: S,
    transport: McpTransport,
}

impl<S> hyper::service::Service<Request<SgBody>> for McpProxy<S>
where
    S: hyper::service::Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn call(&self, mut request: Request<SgBody>) -> Self::Future {
        if let Err(response) = validate_request(&request, self.transport) {
            return Box::pin(async move { Ok(response) });
        }

        let session_id_present = request.headers().contains_key("Mcp-Session-Id");
        request.extensions_mut().insert(McpProxyMeta {
            transport: self.transport.as_str().to_string(),
            route_type: "MCPRoute",
            session_id_present,
        });
        if let Some(telemetry) = request.extensions().get::<TelemetryContext>() {
            telemetry.insert("mcp.transport", self.transport.as_str());
            telemetry.insert("mcp.route_type", "MCPRoute");
            telemetry.insert("mcp.session_id_present", session_id_present.to_string());
        }

        Box::pin(self.inner.call(request))
    }
}

/// Validates only the HTTP transport envelope; JSON-RPC payloads remain opaque to the gateway.
fn validate_request(request: &Request<SgBody>, transport: McpTransport) -> Result<(), Response<SgBody>> {
    match *request.method() {
        Method::GET => {
            if accepts(request, "text/event-stream") {
                Ok(())
            } else {
                Err(protocol_error(StatusCode::NOT_ACCEPTABLE, "MCP GET requires Accept: text/event-stream"))
            }
        }
        Method::POST => {
            if !content_type_is_json(request) {
                return Err(protocol_error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "MCP POST requires Content-Type: application/json"));
            }
            let valid_accept = match transport {
                McpTransport::StreamableHttp => accepts(request, "application/json") && accepts(request, "text/event-stream"),
                McpTransport::LegacySse => true,
            };
            if valid_accept {
                Ok(())
            } else {
                Err(protocol_error(StatusCode::NOT_ACCEPTABLE, "MCP POST does not accept the required response media types"))
            }
        }
        Method::DELETE if transport == McpTransport::StreamableHttp => Ok(()),
        _ => Err(protocol_error(StatusCode::METHOD_NOT_ALLOWED, "MCP transport only supports GET and POST")),
    }
}

/// Checks a comma-separated Accept header without retaining its value in telemetry or logs.
fn accepts(request: &Request<SgBody>, expected: &str) -> bool {
    request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value.split(',').any(|item| {
                let media_type = item.split(';').next().unwrap_or_default().trim();
                media_type.eq_ignore_ascii_case(expected) || media_type == "*/*"
            })
        })
        .unwrap_or(false)
}

/// Accepts JSON media types with optional parameters, for example `application/json; charset=utf-8`.
fn content_type_is_json(request: &Request<SgBody>) -> bool {
    request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().trim().eq_ignore_ascii_case("application/json"))
        .unwrap_or(false)
}

/// Builds a small protocol error response without reflecting request headers or body content.
fn protocol_error(status: StatusCode, message: &'static str) -> Response<SgBody> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(SgBody::full(format!(r#"{{"error":"{message}"}}"#)))
        .expect("MCP protocol error response is valid")
}
