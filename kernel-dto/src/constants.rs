#[cfg(feature = "k8s")]
pub const GATEWAY_ANNOTATION_REDIS_URL: &str = "redis_url";
#[cfg(feature = "k8s")]
pub const GATEWAY_ANNOTATION_LOG_LEVEL: &str = "log_level";
#[cfg(feature = "k8s")]
pub const GATEWAY_ANNOTATION_LANGUAGE: &str = "lang";
#[cfg(feature = "k8s")]
pub const GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION: &str = "ignore_tls_verification";

#[cfg(feature = "k8s")]
pub const GATEWAY_CLASS_NAME: &str = "spacegate";
#[cfg(feature = "k8s")]
pub const DEFAULT_NAMESPACE: &str = "default";
