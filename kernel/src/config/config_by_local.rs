use std::time::Duration;

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    log,
    tokio::{self, sync::Mutex, time},
    TardisFuns,
};

use crate::{do_startup, functions::http_route, shutdown};

use super::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use lazy_static::lazy_static;

lazy_static! {
    static ref MD5_CACHE: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
}

pub async fn init(conf_path: &str, check_interval_sec: u64) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    let gateway_config_path = format!("{conf_path}/gateway.json");
    let routes_config_path = format!("{conf_path}/routes");

    let (config, _, _) = fetch_configs(&gateway_config_path, &routes_config_path).await?;

    tardis::tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(check_interval_sec));
        loop {
            {
                log::trace!("[SG.Config] Config change check");
                let (config, gateway_config_changed, routes_config_changed) =
                    fetch_configs(&gateway_config_path, &routes_config_path).await.expect("[SG.Config] init Failed to fetch configs");
                if gateway_config_changed {
                    let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
                    shutdown(&gateway_config.name).await.expect("[SG.Config] shutdown failed");
                    do_startup(gateway_config, http_route_configs).await.expect("[SG.Config] re-startup failed");
                } else if routes_config_changed {
                    let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
                    http_route::init(gateway_config, http_route_configs).await.expect("[SG.Config] route re-init failed");
                }
            }
            interval.tick().await;
        }
    });
    Ok(vec![config.expect("[SG.Config] config is None")])
}

async fn fetch_configs(gateway_config_path: &str, routes_config_path: &str) -> TardisResult<(Option<(SgGateway, Vec<SgHttpRoute>)>, bool, bool)> {
    let gateway_config_content = tokio::fs::read_to_string(&gateway_config_path).await?;
    if gateway_config_content.is_empty() {
        return Err(TardisError::not_found(&format!("[SG.Config] Gateway Config not found in {gateway_config_path} file"), ""));
    }
    let routes_config_content = {
        let mut routes_config_dir = tokio::fs::read_dir(&routes_config_path).await?;
        let mut routes_config_content = Vec::new();
        while let Some(route_config_dir) = routes_config_dir.next_entry().await? {
            routes_config_content.push(tokio::fs::read_to_string(&route_config_dir.path()).await?);
        }
        if routes_config_content.is_empty() {
            return Err(TardisError::not_found(
                &format!("[SG.Config] Routes Config not found in {routes_config_path} directory"),
                "",
            ));
        }
        routes_config_content
    };
    let gateway_config_md5 = TardisFuns::crypto.digest.md5(&gateway_config_content)?;
    let routes_config_md5 = TardisFuns::crypto.digest.md5(routes_config_content.join("\r\n").as_str())?;

    let mut md5_cache = MD5_CACHE.lock().await;
    let gateway_config_changed = gateway_config_md5 != md5_cache.0;
    let http_route_configs_changed = routes_config_md5 != md5_cache.1;
    *md5_cache = (gateway_config_md5, routes_config_md5);

    if gateway_config_changed || http_route_configs_changed {
        let gateway_config = tardis::TardisFuns::json
            .str_to_obj::<SgGateway>(&gateway_config_content)
            .map_err(|e| TardisError::internal_error(&format!("[SG.Config] parse gateway config error: {e}"), ""))?;
        let http_route_configs = routes_config_content
            .iter()
            .map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(v).map_err(|e| TardisError::internal_error(&format!("[SG.Config] parse route config error: {e}"), "")))
            .collect::<TardisResult<Vec<SgHttpRoute>>>()?;
        Ok((Some((gateway_config, http_route_configs)), gateway_config_changed, http_route_configs_changed))
    } else {
        Ok((None, gateway_config_changed, http_route_configs_changed))
    }
}
