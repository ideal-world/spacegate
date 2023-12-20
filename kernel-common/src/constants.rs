#[cfg(feature = "k8s")]
pub mod k8s_constants;

pub const RAW_HTTP_ROUTE_KIND: &str = "raw.http.route.kind";
pub const RAW_HTTP_ROUTE_KIND_DEFAULT: &str = "HTTPRoute";
pub const RAW_HTTP_ROUTE_KIND_SPACEROUTE: &str = "HTTPSpaceroute";

pub const ANNOTATION_RESOURCE_PRIORITY: &str = "priority";

pub const BANCKEND_KIND_EXTERNAL: &str = "External";
pub const BANCKEND_KIND_EXTERNAL_HTTP: &str = "ExternalHttp";
pub const BANCKEND_KIND_EXTERNAL_HTTPS: &str = "ExternalHttps";
