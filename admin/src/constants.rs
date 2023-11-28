use lazy_static::lazy_static;
use std::collections::HashMap;

pub const DOMAIN_CODE: &str = "admin";

pub const KUBE_VO_NAMESPACE: &str = "spacegate";

pub const GATEWAY_CONFIG_NAME: &str = "gateway-config";
pub const TLS_CONFIG_NAME: &str = "tls-config";
pub const PLUGIN_CONFIG_NAME: &str = "plugin-config";
pub const ROUTE_CONFIG_NAME: &str = "route-config";
pub const BACKEND_REF_CONFIG_NAME: &str = "backend-ref-config";
pub const CLUSTER_CONFIG_NAME: &str = "k8s-cluster-config";

pub const GATEWAY_TYPE: &str = "Gateway";
pub const TLS_CONFIG_TYPE: &str = "TlsConfig";
pub const PLUGIN_TYPE: &str = "Plugin";
pub const ROUTE_TYPE: &str = "Route";
pub const BACKEND_REF_TYPE: &str = "BackendRef";
pub const CLUSTER_TYPE: &str = "Cluster";

lazy_static! {
    pub static ref TYPE_CONFIG_NAME_MAP: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();
        map.insert(GATEWAY_TYPE, GATEWAY_CONFIG_NAME);
        map.insert(TLS_CONFIG_TYPE, TLS_CONFIG_NAME);
        map.insert(PLUGIN_TYPE, PLUGIN_CONFIG_NAME);
        map.insert(ROUTE_TYPE, ROUTE_CONFIG_NAME);
        map.insert(BACKEND_REF_TYPE, BACKEND_REF_CONFIG_NAME);
        map.insert(CLUSTER_TYPE, CLUSTER_CONFIG_NAME);
        map
    };
}
