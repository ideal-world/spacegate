use std::{num::NonZeroUsize, time::Duration};

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    cache::{AsyncCommands, AsyncIter},
    log,
    lru::LruCache,
    tokio::{sync::Mutex, time},
};

use crate::{do_startup, shutdown, update_route};

use super::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};
use lazy_static::lazy_static;

lazy_static! {
    static ref CHANGE_CACHE: Mutex<LruCache<String, bool>> = Mutex::new(LruCache::new(NonZeroUsize::new(100).expect("NonZeroUsize::new failed")));
}

// hash: {gateway name} -> {gateway config}
const CONF_GATEWAY_KEY: &str = "sg:conf:gateway";
// list: {gateway name} -> {vec<http route config>}
const CONF_HTTP_ROUTE_KEY: &str = "sg:conf:route:http:";
// string: {timestamp}##{changed obj}##{changed gateway name} -> None
const CONF_CHANGE_TRIGGER: &str = "sg:conf:change:trigger:";

pub async fn init(conf_url: &str, check_interval_sec: u64) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    crate::cache_client::init("", conf_url).await?;
    let cache_client = crate::cache_client::get("").await?;
    let mut config = Vec::new();
    let gateway_configs = cache_client.hgetall(CONF_GATEWAY_KEY).await?;
    if gateway_configs.is_empty() {
        return Err(TardisError::not_found(&format!("[SG.Config] Gateway Config not found in {CONF_GATEWAY_KEY}"), ""));
    }
    let gateway_configs = gateway_configs
        .into_values()
        .map(|v| tardis::TardisFuns::json.str_to_obj::<SgGateway>(&v).map_err(|e| TardisError::format_error(&format!("[SG.Config] Gateway Config parse error {}", e), "")))
        .collect::<TardisResult<Vec<SgGateway>>>()?;
    for gateway_config in gateway_configs {
        let http_route_configs = cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_config.name)).await?;
        let http_route_configs = http_route_configs
            .into_iter()
            .map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).map_err(|e| TardisError::format_error(&format!("[SG.Config] Http Route Config parse error {}", e), "")))
            .collect::<TardisResult<Vec<SgHttpRoute>>>()?;
        config.push((gateway_config, http_route_configs));
    }
    tardis::tokio::task::spawn_local(async move {
        let mut interval = time::interval(Duration::from_secs(check_interval_sec));
        loop {
            {
                log::trace!("[SG.Config] Config change check");
                let mut cache_cmd = cache_client.cmd().await.expect("[SG.Config] cache_client cmd failed");
                let mut key_iter: AsyncIter<String> = cache_cmd.scan_match(&format!("{}*", CONF_CHANGE_TRIGGER)).await.expect("[SG.Config] cache_client scan_match failed");

                while let Some(changed_key) = key_iter.next_item().await {
                    let changed_key = changed_key.strip_prefix(CONF_CHANGE_TRIGGER).expect("[SG.Config] strip_prefix failed");
                    let f = changed_key.split("##").collect::<Vec<_>>();
                    let unique = f[0];
                    let mut lock = CHANGE_CACHE.lock().await;
                    if lock.put(unique.to_string(), true).is_some() {
                        continue;
                    }
                    let changed_obj = f[1];
                    let changed_gateway_name = f[2];
                    log::trace!("[SG.Config] Config change found, {changed_obj}: {changed_gateway_name}");

                    if let Some(gateway_config) = cache_client.hget(CONF_GATEWAY_KEY, changed_gateway_name).await.expect("[SG.Config] cache_client hget failed") {
                        log::trace!("[SG.Config] new gateway config {gateway_config}");

                        // Added or modified
                        let gateway_config = tardis::TardisFuns::json.str_to_obj::<SgGateway>(&gateway_config).expect("[SG.Config] Gateway Config parse error");
                        let http_route_configs =
                            cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_config.name)).await.expect("[SG.Config] cache_client lrangeall failed");
                        let http_route_configs = http_route_configs
                            .into_iter()
                            .map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).expect("[SG.config] Route config parse error"))
                            .collect::<Vec<SgHttpRoute>>();
                        match changed_obj {
                            "gateway" => {
                                shutdown(changed_gateway_name).await.expect("[SG.Config] shutdown failed");
                                do_startup(gateway_config, http_route_configs).await.expect("[SG.Config] re-startup failed");
                            }
                            "httproute" => {
                                update_route(changed_gateway_name, http_route_configs).await.expect("[SG.Config] fail to update route config");
                            }
                            _ => {}
                        }
                    } else {
                        // Removed
                        if changed_obj == "gateway" {
                            shutdown(changed_gateway_name).await.expect("[SG.Config] shutdown failed");
                        }
                    }
                }
            }
            interval.tick().await;
        }
    });
    Ok(config)
}
