use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hyper::{header, Request, Response, StatusCode, Version};
use opentelemetry::{global, KeyValue};

use crate::{extension::GatewayName, SgBody};

#[derive(Debug, Clone, Default)]
pub struct TelemetryContext {
    fields: Arc<Mutex<BTreeMap<String, String>>>,
}

#[derive(Debug, Clone, Default)]
pub struct AccessLogContext {
    fields: Arc<Mutex<AccessLogContextFields>>,
}

#[derive(Debug, Clone, Default)]
struct AccessLogContextFields {
    route_name: Option<String>,
    upstream_host: Option<String>,
}

pub const MAX_TELEMETRY_KEY_LEN: usize = 128;
pub const MAX_TELEMETRY_VALUE_LEN: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryError {
    EmptyKey,
    MissingNamespace,
    ReservedPrefix,
    InvalidKey,
    KeyTooLong,
    ValueTooLong,
}

pub fn validate_telemetry_key(key: &str) -> Result<(), TelemetryError> {
    if key.is_empty() {
        return Err(TelemetryError::EmptyKey);
    }
    if key.len() > MAX_TELEMETRY_KEY_LEN {
        return Err(TelemetryError::KeyTooLong);
    }
    if !key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')) {
        return Err(TelemetryError::InvalidKey);
    }
    if !key.contains('.') {
        return Err(TelemetryError::MissingNamespace);
    }
    if ["http.", "net.", "gateway.", "spacegate.", "otel."].iter().any(|prefix| key.starts_with(prefix)) {
        return Err(TelemetryError::ReservedPrefix);
    }
    Ok(())
}

pub fn validate_telemetry_value(value: &str) -> Result<(), TelemetryError> {
    if value.len() > MAX_TELEMETRY_VALUE_LEN {
        return Err(TelemetryError::ValueTooLong);
    }
    Ok(())
}

impl TelemetryContext {
    pub fn insert(&self, key: impl Into<String>, value: impl Into<String>) {
        let Ok(mut fields) = self.fields.lock() else {
            return;
        };
        fields.insert(key.into(), value.into());
    }

    pub fn insert_checked(&self, key: impl Into<String>, value: impl ToString) -> Result<(), TelemetryError> {
        let key = key.into();
        let value = value.to_string();
        validate_telemetry_key(&key)?;
        validate_telemetry_value(&value)?;
        let Ok(mut fields) = self.fields.lock() else {
            return Ok(());
        };
        fields.insert(key, value);
        Ok(())
    }

    pub fn insert_namespaced(&self, namespace: &str, key: &str, value: impl ToString) -> Result<(), TelemetryError> {
        self.insert_checked(format!("{namespace}.{key}"), value)
    }

    pub fn snapshot(&self) -> BTreeMap<String, String> {
        self.fields.lock().map(|fields| fields.clone()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.lock().map(|fields| fields.is_empty()).unwrap_or(true)
    }
}

impl AccessLogContext {
    pub fn set_route_name(&self, value: impl Into<String>) {
        let Ok(mut fields) = self.fields.lock() else {
            return;
        };
        fields.route_name = Some(value.into());
    }

    pub fn set_upstream_host(&self, value: impl Into<String>) {
        let Ok(mut fields) = self.fields.lock() else {
            return;
        };
        fields.upstream_host = Some(value.into());
    }

    pub fn route_name(&self) -> String {
        self.fields.lock().ok().and_then(|fields| fields.route_name.clone()).unwrap_or_default()
    }

    pub fn upstream_host(&self) -> String {
        self.fields.lock().ok().and_then(|fields| fields.upstream_host.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct HttpMetricLabels {
    pub gateway: String,
    pub method: String,
    pub status_code: String,
    pub protocol_name: String,
    pub protocol_version: String,
    pub request_body_size: Option<u64>,
    pub response_body_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogFields {
    pub gateway: String,
    pub method: String,
    pub path: String,
    pub host: String,
    pub client_ip: String,
    pub x_forwarded_for: String,
    pub user_agent: String,
    pub authority: String,
    pub downstream_remote_address: String,
    pub route_name: String,
    pub upstream_host: String,
    pub trace_id: String,
    pub protocol_name: String,
    pub protocol_version: String,
    pub status_code: u16,
    pub request_id: String,
    pub peer_addr: String,
    pub duration_ms: u64,
    pub request_body_size: Option<u64>,
    pub response_body_size: Option<u64>,
    pub telemetry: BTreeMap<String, String>,
}

pub fn http_metric_labels(req: &Request<SgBody>, resp: &Response<SgBody>) -> HttpMetricLabels {
    HttpMetricLabels {
        gateway: req.extensions().get::<GatewayName>().map(|g| g.to_string()).unwrap_or_else(|| "unknown".to_string()),
        method: req.method().as_str().to_string(),
        status_code: resp.status().as_u16().to_string(),
        protocol_name: "http".to_string(),
        protocol_version: http_protocol_version(req.version()),
        request_body_size: content_length(req.headers()),
        response_body_size: content_length(resp.headers()),
    }
}

pub fn access_log_fields(
    gateway: impl Into<String>,
    method: impl Into<String>,
    path: impl Into<String>,
    host: impl Into<String>,
    client_ip: impl Into<String>,
    x_forwarded_for: impl Into<String>,
    user_agent: impl Into<String>,
    authority: impl Into<String>,
    downstream_remote_address: impl Into<String>,
    route_name: impl Into<String>,
    upstream_host: impl Into<String>,
    trace_id: impl Into<String>,
    protocol_version: impl Into<String>,
    status_code: StatusCode,
    request_id: impl Into<String>,
    peer_addr: impl Into<String>,
    duration: Duration,
    request_body_size: Option<u64>,
    response_body_size: Option<u64>,
    telemetry: BTreeMap<String, String>,
) -> AccessLogFields {
    AccessLogFields {
        gateway: gateway.into(),
        method: method.into(),
        path: path.into(),
        host: host.into(),
        client_ip: client_ip.into(),
        x_forwarded_for: x_forwarded_for.into(),
        user_agent: user_agent.into(),
        authority: authority.into(),
        downstream_remote_address: downstream_remote_address.into(),
        route_name: route_name.into(),
        upstream_host: upstream_host.into(),
        trace_id: trace_id.into(),
        protocol_name: "http".to_string(),
        protocol_version: protocol_version.into(),
        status_code: status_code.as_u16(),
        request_id: request_id.into(),
        peer_addr: peer_addr.into(),
        duration_ms: duration.as_millis() as u64,
        request_body_size,
        response_body_size,
        telemetry,
    }
}

pub fn telemetry_json(fields: &BTreeMap<String, String>) -> String {
    serde_json::to_string(fields).unwrap_or_else(|_| "{}".to_string())
}

pub fn content_length(headers: &hyper::HeaderMap) -> Option<u64> {
    headers.get(header::CONTENT_LENGTH)?.to_str().ok()?.parse().ok()
}

pub fn header_value(headers: &hyper::HeaderMap, name: impl AsRef<str>) -> String {
    headers.get(name.as_ref()).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string()
}

pub fn first_x_forwarded_for(headers: &hyper::HeaderMap) -> Option<String> {
    header_value(headers, "x-forwarded-for").split(',').map(str::trim).find(|value| !value.is_empty()).map(str::to_string)
}

pub fn client_ip(headers: &hyper::HeaderMap, peer_addr: std::net::SocketAddr) -> String {
    first_x_forwarded_for(headers).unwrap_or_else(|| peer_addr.ip().to_string())
}

pub fn record_http_server_metrics(req: &Request<SgBody>, resp: &Response<SgBody>, duration: Duration) {
    let labels = http_metric_labels(req, resp);
    record_http_server_metrics_with_labels(labels, duration, resp.status().is_server_error() || resp.status().is_client_error());
}

pub fn record_http_server_metrics_with_labels(labels: HttpMetricLabels, duration: Duration, is_error: bool) {
    let error_class = status_error_class_from_code(&labels.status_code);
    let attrs = [
        KeyValue::new("gateway", labels.gateway),
        KeyValue::new("http.request.method", labels.method),
        KeyValue::new("http.response.status_code", labels.status_code),
        KeyValue::new("network.protocol.name", labels.protocol_name),
        KeyValue::new("network.protocol.version", labels.protocol_version),
    ];
    let instruments = http_instruments();
    instruments.requests.add(1, &attrs);
    instruments.duration.record(duration.as_secs_f64(), &attrs);
    if let Some(size) = labels.request_body_size {
        instruments.request_body_size.record(size, &attrs);
    }
    if let Some(size) = labels.response_body_size {
        instruments.response_body_size.record(size, &attrs);
    }
    if is_error {
        instruments.errors.add(1, &attrs);
    }
    match error_class {
        Some(HttpErrorClass::Client) => instruments.errors_4xx.add(1, &attrs),
        Some(HttpErrorClass::Server) => instruments.errors_5xx.add(1, &attrs),
        None => {}
    }
}

pub fn record_http_server_active_request(labels: HttpMetricLabels, delta: i64) {
    let attrs = [
        KeyValue::new("gateway", labels.gateway),
        KeyValue::new("http.request.method", labels.method),
        KeyValue::new("network.protocol.name", labels.protocol_name),
        KeyValue::new("network.protocol.version", labels.protocol_version),
    ];
    http_instruments().active_requests.add(delta, &attrs);
}

#[derive(Debug)]
struct HttpInstruments {
    requests: opentelemetry::metrics::Counter<u64>,
    errors: opentelemetry::metrics::Counter<u64>,
    errors_4xx: opentelemetry::metrics::Counter<u64>,
    errors_5xx: opentelemetry::metrics::Counter<u64>,
    active_requests: opentelemetry::metrics::UpDownCounter<i64>,
    duration: opentelemetry::metrics::Histogram<f64>,
    request_body_size: opentelemetry::metrics::Histogram<u64>,
    response_body_size: opentelemetry::metrics::Histogram<u64>,
}

fn http_instruments() -> &'static HttpInstruments {
    static INSTRUMENTS: OnceLock<HttpInstruments> = OnceLock::new();
    INSTRUMENTS.get_or_init(|| {
        let meter = global::meter("spacegate_kernel");
        HttpInstruments {
            requests: meter.u64_counter("http.server.requests").build(),
            errors: meter.u64_counter("http.server.errors").build(),
            errors_4xx: meter.u64_counter("http.server.errors.4xx").build(),
            errors_5xx: meter.u64_counter("http.server.errors.5xx").build(),
            active_requests: meter.i64_up_down_counter("http.server.active_requests").with_unit("{request}").build(),
            duration: meter.f64_histogram("http.server.request.duration").with_unit("s").build(),
            request_body_size: meter.u64_histogram("http.server.request.body.size").with_unit("By").build(),
            response_body_size: meter.u64_histogram("http.server.response.body.size").with_unit("By").build(),
        }
    })
}

pub fn http_protocol_version(version: Version) -> String {
    match version {
        Version::HTTP_10 => "1.0",
        Version::HTTP_11 => "1.1",
        Version::HTTP_2 => "2",
        Version::HTTP_3 => "3",
        _ => "unknown",
    }
    .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErrorClass {
    Client,
    Server,
}

pub fn status_error_class(status: StatusCode) -> Option<HttpErrorClass> {
    if status.is_client_error() {
        Some(HttpErrorClass::Client)
    } else if status.is_server_error() {
        Some(HttpErrorClass::Server)
    } else {
        None
    }
}

fn status_error_class_from_code(status_code: &str) -> Option<HttpErrorClass> {
    StatusCode::from_u16(status_code.parse().ok()?).ok().and_then(status_error_class)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use hyper::{header, Request, Response, StatusCode};

    use crate::{extension::GatewayName, observability::http_metric_labels, SgBody};

    #[test]
    fn http_metric_labels_do_not_include_path() {
        let mut req = Request::builder().method("GET").uri("/users/123?token=secret").body(SgBody::empty()).expect("request");
        req.extensions_mut().insert(GatewayName::new("gw-a"));
        let resp = Response::builder().status(StatusCode::OK).body(SgBody::empty()).expect("response");

        let labels = http_metric_labels(&req, &resp);

        assert_eq!(labels.gateway, "gw-a");
        assert_eq!(labels.method, "GET");
        assert_eq!(labels.status_code, "200");
        assert!(!format!("{labels:?}").contains("/users/123"));
    }

    #[test]
    fn http_metric_labels_use_protocol_name_and_version() {
        let req = Request::builder().version(hyper::Version::HTTP_2).body(SgBody::empty()).expect("request");
        let resp = Response::builder().status(StatusCode::OK).body(SgBody::empty()).expect("response");

        let labels = http_metric_labels(&req, &resp);

        assert_eq!(labels.protocol_name, "http");
        assert_eq!(labels.protocol_version, "2");
    }

    #[test]
    fn http_protocol_version_maps_known_versions() {
        assert_eq!(super::http_protocol_version(hyper::Version::HTTP_10), "1.0");
        assert_eq!(super::http_protocol_version(hyper::Version::HTTP_11), "1.1");
        assert_eq!(super::http_protocol_version(hyper::Version::HTTP_2), "2");
        assert_eq!(super::http_protocol_version(hyper::Version::HTTP_3), "3");
    }

    #[test]
    fn status_error_classifies_4xx_and_5xx_only() {
        assert_eq!(super::status_error_class(StatusCode::OK), None);
        assert_eq!(super::status_error_class(StatusCode::BAD_REQUEST), Some(super::HttpErrorClass::Client));
        assert_eq!(super::status_error_class(StatusCode::INTERNAL_SERVER_ERROR), Some(super::HttpErrorClass::Server));
    }

    #[test]
    fn access_log_fields_include_stable_request_data_and_telemetry() {
        let telemetry = BTreeMap::from([("ai.asset_id".to_string(), "deepseek-chat".to_string()), ("ai.total_tokens".to_string(), "37".to_string())]);

        let fields = super::access_log_fields(
            "gw-a",
            "POST",
            "/api/v1/model/deepseek-chat",
            "example.local",
            "203.0.113.10",
            "203.0.113.10, 10.0.0.1",
            "curl/8.7.1",
            "example.local",
            "127.0.0.1:12345",
            "model-route",
            "model.default.svc.cluster.local",
            "4bf92f3577b34da6a3ce929d0e0e4736",
            "1.1",
            StatusCode::OK,
            "req-1",
            "127.0.0.1:12345",
            Duration::from_millis(42),
            Some(12),
            Some(34),
            telemetry,
        );

        assert_eq!(fields.gateway, "gw-a");
        assert_eq!(fields.method, "POST");
        assert_eq!(fields.client_ip, "203.0.113.10");
        assert_eq!(fields.x_forwarded_for, "203.0.113.10, 10.0.0.1");
        assert_eq!(fields.user_agent, "curl/8.7.1");
        assert_eq!(fields.authority, "example.local");
        assert_eq!(fields.downstream_remote_address, "127.0.0.1:12345");
        assert_eq!(fields.route_name, "model-route");
        assert_eq!(fields.upstream_host, "model.default.svc.cluster.local");
        assert_eq!(fields.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(fields.status_code, 200);
        assert_eq!(fields.duration_ms, 42);
        assert_eq!(fields.request_body_size, Some(12));
        assert_eq!(fields.response_body_size, Some(34));
        assert_eq!(fields.telemetry.get("ai.asset_id").map(String::as_str), Some("deepseek-chat"));
    }

    #[test]
    fn http_metric_labels_include_body_sizes_from_content_length() {
        let req = Request::builder().method("POST").header(header::CONTENT_LENGTH, "123").body(SgBody::empty()).expect("request");
        let resp = Response::builder().status(StatusCode::OK).header(header::CONTENT_LENGTH, "456").body(SgBody::empty()).expect("response");

        let labels = http_metric_labels(&req, &resp);

        assert_eq!(labels.request_body_size, Some(123));
        assert_eq!(labels.response_body_size, Some(456));
    }

    #[test]
    fn http_metric_labels_ignore_invalid_body_sizes() {
        let req = Request::builder().header(header::CONTENT_LENGTH, "chunked").body(SgBody::empty()).expect("request");
        let resp = Response::builder().header(header::CONTENT_LENGTH, "-1").body(SgBody::empty()).expect("response");

        let labels = http_metric_labels(&req, &resp);

        assert_eq!(labels.request_body_size, None);
        assert_eq!(labels.response_body_size, None);
    }

    #[test]
    fn client_ip_prefers_first_x_forwarded_for_value() {
        let req = Request::builder().header("x-forwarded-for", "203.0.113.10, 10.0.0.1").body(SgBody::empty()).expect("request");
        let peer = "127.0.0.1:12345".parse().expect("peer");

        assert_eq!(super::client_ip(req.headers(), peer), "203.0.113.10");
    }

    #[test]
    fn client_ip_falls_back_to_peer_ip() {
        let req = Request::builder().body(SgBody::empty()).expect("request");
        let peer = "127.0.0.1:12345".parse().expect("peer");

        assert_eq!(super::client_ip(req.headers(), peer), "127.0.0.1");
    }

    #[test]
    fn telemetry_context_collects_plugin_fields() {
        let context = super::TelemetryContext::default();

        context.insert("ai.asset_id", "deepseek-chat");
        context.insert("ai.total_tokens", "37");

        let fields = context.snapshot();
        assert_eq!(fields.get("ai.asset_id").map(String::as_str), Some("deepseek-chat"));
        assert_eq!(fields.get("ai.total_tokens").map(String::as_str), Some("37"));
    }

    #[test]
    fn access_log_context_keeps_route_and_backend_for_early_responses() {
        let context = super::AccessLogContext::default();

        context.set_route_name("model-route");
        context.set_upstream_host("model.default.svc.cluster.local");

        assert_eq!(context.route_name(), "model-route");
        assert_eq!(context.upstream_host(), "model.default.svc.cluster.local");
    }

    #[test]
    fn telemetry_key_validation_accepts_namespaced_keys() {
        assert!(super::validate_telemetry_key("ai.total_tokens").is_ok());
        assert!(super::validate_telemetry_key("mcp.tool-name").is_ok());
        assert!(super::validate_telemetry_key("auth.api_key_hash").is_ok());
    }

    #[test]
    fn telemetry_key_validation_rejects_bad_keys() {
        assert_eq!(super::validate_telemetry_key(""), Err(super::TelemetryError::EmptyKey));
        assert_eq!(super::validate_telemetry_key("total_tokens"), Err(super::TelemetryError::MissingNamespace));
        assert_eq!(super::validate_telemetry_key("ai total_tokens"), Err(super::TelemetryError::InvalidKey));
        assert_eq!(super::validate_telemetry_key("http.status_code"), Err(super::TelemetryError::ReservedPrefix));
        assert_eq!(super::validate_telemetry_key("spacegate.internal"), Err(super::TelemetryError::ReservedPrefix));
    }

    #[test]
    fn telemetry_value_validation_rejects_oversized_values() {
        let value = "x".repeat(super::MAX_TELEMETRY_VALUE_LEN + 1);
        assert_eq!(super::validate_telemetry_value(&value), Err(super::TelemetryError::ValueTooLong));
    }

    #[test]
    fn telemetry_context_checked_insert_rejects_invalid_key_without_mutating_context() {
        let context = super::TelemetryContext::default();

        let result = context.insert_checked("total_tokens", "37");

        assert_eq!(result, Err(super::TelemetryError::MissingNamespace));
        assert!(context.snapshot().is_empty());
    }

    #[test]
    fn telemetry_context_namespaced_insert_builds_stable_key() {
        let context = super::TelemetryContext::default();

        context.insert_namespaced("ai", "total_tokens", 37).expect("insert");

        let fields = context.snapshot();
        assert_eq!(fields.get("ai.total_tokens").map(String::as_str), Some("37"));
    }

    #[test]
    fn telemetry_json_serializes_plugin_defined_fields() {
        let fields = BTreeMap::from([
            ("ai.asset_id".to_string(), "deepseek-chat".to_string()),
            ("ai.total_tokens".to_string(), "37".to_string()),
            ("mcp.tool".to_string(), "search".to_string()),
        ]);

        let json = super::telemetry_json(&fields);

        assert!(json.contains("\"ai.asset_id\":\"deepseek-chat\""));
        assert!(json.contains("\"ai.total_tokens\":\"37\""));
        assert!(json.contains("\"mcp.tool\":\"search\""));
    }
}
