pub const KUBE_OBJECT_INSTANCE: &str = "app.kubernetes.io/instance";

pub const GATEWAY_CONTROLLER_NAME: &str = "spacegate.idealworld.group/spacegate-controller";
pub const GATEWAY_CLASS_NAME: &str = "spacegate";
pub const GATEWAY_DEFAULT_INSTANCE: &str = "spacegate.spacegate";
pub const GATEWAY_ANNOTATION_REDIS_URL: &str = "redis_url";
pub const GATEWAY_ANNOTATION_LOG_LEVEL: &str = "log_level";
pub const GATEWAY_ANNOTATION_LANGUAGE: &str = "lang";
pub const GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION: &str = "ignore_tls_verification";
pub const GATEWAY_ANNOTATION_ENABLE_X_REQUEST_ID: &str = "enable_x_request_id";
pub const GATEWAY_ANNOTATION_OTEL_ENABLED: &str = "spacegate.io/otel-enabled";
pub const GATEWAY_ANNOTATION_OTEL_SERVICE_NAME: &str = "spacegate.io/otel-service-name";
pub const GATEWAY_ANNOTATION_OTEL_ENDPOINT: &str = "spacegate.io/otel-endpoint";
pub const GATEWAY_ANNOTATION_OTEL_PROTOCOL: &str = "spacegate.io/otel-protocol";
pub const GATEWAY_ANNOTATION_OTEL_TRACES_ENABLED: &str = "spacegate.io/otel-traces-enabled";
pub const GATEWAY_ANNOTATION_OTEL_TRACES_SAMPLE_RATIO: &str = "spacegate.io/otel-traces-sample-ratio";
pub const GATEWAY_ANNOTATION_OTEL_METRICS_ENABLED: &str = "spacegate.io/otel-metrics-enabled";
pub const GATEWAY_ANNOTATION_OTEL_METRICS_EXPORT_INTERVAL_MS: &str = "spacegate.io/otel-metrics-export-interval-ms";
pub const GATEWAY_ANNOTATION_OTEL_LOGS_ENABLED: &str = "spacegate.io/otel-logs-enabled";
pub const GATEWAY_ANNOTATION_OTEL_LOGS_LEVEL: &str = "spacegate.io/otel-logs-level";

pub const SG_FILTER_KIND: &str = "sgfilter";
pub const DEFAULT_NAMESPACE: &str = "default";
pub const ANNOTATION_RESOURCE_PRIORITY: &str = "priority";

pub const RAW_HTTP_ROUTE_KIND: &str = "raw.http.route.kind";
pub const RAW_HTTP_ROUTE_KIND_DEFAULT: &str = "HTTPRoute";
pub const RAW_HTTP_ROUTE_KIND_SPACEROUTE: &str = "HTTPSpaceroute";

pub const DEFAULT_API_PORT: u16 = 9876;
