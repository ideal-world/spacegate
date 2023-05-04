use tardis::{basic::result::TardisResult, log};

use crate::config::gateway_dto::SgGateway;

use self::http_route_dto::SgHttpRoute;

#[cfg(feature = "k8s")]
pub mod config_by_k8s;
pub mod config_by_local;
#[cfg(feature = "cache")]
pub mod config_by_redis;
pub mod gateway_dto;
pub mod http_route_dto;
pub mod plugin_filter_dto;

pub async fn init(k8s_mode: bool, namespace_or_conf_uri: String, check_interval_sec: Option<u64>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    log::info!(
        "[SG.Config] Config initialization mode: {}",
        if k8s_mode {
            format!("kubernetes, with namespaces: {namespace_or_conf_uri}")
        } else {
            format!("non-kubernetes, with uri: {namespace_or_conf_uri}")
        }
    );
    if k8s_mode {
        #[cfg(feature = "k8s")]
        {
            config_by_k8s::init(&namespace_or_conf_uri).await
        }
        #[cfg(not(feature = "k8s"))]
        {
            Err(tardis::basic::error::TardisError::not_found(
                "[SG.Config] The current compilation mode does not support k8s",
                "",
            ))
        }
    } else {
        #[cfg(feature = "cache")]
        {
            config_by_redis::init(&namespace_or_conf_uri, check_interval_sec.unwrap_or(10)).await
        }
        #[cfg(not(feature = "cache"))]
        {
            config_by_local::init(&namespace_or_conf_uri, check_interval_sec.unwrap_or(10)).await
        }
    }
}
