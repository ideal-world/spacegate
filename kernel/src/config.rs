use tardis::{
    basic::{error::TardisError, result::TardisResult},
    log,
};

use crate::config::gateway_dto::SgGateway;

use self::http_route_dto::SgHttpRoute;

pub mod config_by_k8s;
#[cfg(feature = "cache")]
pub mod config_by_redis;
pub mod gateway_dto;
pub mod http_route_dto;
pub mod plugin_filter_dto;

pub async fn init(k8s_mode: bool, ext_conf_url: Option<String>, check_interval_sec: Option<u64>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    log::info!(
        "[SG.Config] Config initialization mode: {}",
        if k8s_mode {
            "kubernetes".to_string()
        } else {
            format!("non-kubernetes, with url: {ext_conf_url:?}")
        }
    );
    if k8s_mode {
        config_by_k8s::init().await
    } else {
        let ext_conf_url = ext_conf_url.ok_or_else(|| {
            TardisError::bad_request(
                "[SG.Config] In non-kubernetes mode, the configuration information must be passed in to obtain the address",
                "",
            )
        })?;
        #[cfg(feature = "cache")]
        {
            config_by_redis::init(&ext_conf_url, check_interval_sec.unwrap_or(10)).await
        }
        #[cfg(not(feature = "cache"))]
        {
            Err(TardisError::not_found("[SG.Config] Missing [ext_conf_url] configuration address", ""))
        }
    }
}
