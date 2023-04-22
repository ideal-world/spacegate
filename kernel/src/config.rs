use std::collections::HashMap;

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use crate::{config::gateway_dto::SgGateway, functions::cache};

use self::http_route_dto::SgHttpRoute;

pub mod gateway_dto;
pub mod http_route_dto;
pub mod plugin_filter_dto;

const CONF_GATEWAY_KEY: &str = "sg:conf:gateway";
const CONF_HTTP_ROUTE_KEY: &str = "sg:conf:route:http";

pub async fn init(k8s_mode: bool, ext_conf_url: Option<String>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    if k8s_mode {
        init_by_k8s().await
    } else {
        let ext_conf_url = ext_conf_url.ok_or_else(|| {
            TardisError::bad_request(
                "[SG.Config] In non-kubernetes mode, the configuration information must be passed in to obtain the address",
                "",
            )
        })?;
        init_by_native(&ext_conf_url).await
    }
}

async fn init_by_k8s() -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    todo!()
}

async fn init_by_native(ext_conf_url: &str) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    cache::init("", ext_conf_url).await?;
    let cache = cache::get("")?;
    let gateway_confs = cache.hgetall(CONF_GATEWAY_KEY).await?;
    let gateway_confs = gateway_confs.into_iter().map(|(k, v)| TardisFuns::json.str_to_obj::<SgGateway>(&v).unwrap()).collect::<Vec<SgGateway>>();
    let http_route_confs = cache.hgetall(CONF_HTTP_ROUTE_KEY).await?;
    let http_route_confs = http_route_confs.into_iter().map(|(k, v)| TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).unwrap()).collect::<Vec<SgHttpRoute>>();
    let config = gateway_confs
        .into_iter()
        .map(|gateway_conf| {
            let http_route_confs = http_route_confs
                .iter()
                .filter(|http_route_conf| http_route_conf.gateway_name == gateway_conf.name)
                .map(|http_route_conf| http_route_conf.clone())
                .collect::<Vec<SgHttpRoute>>();
            (gateway_conf, http_route_confs)
        })
        .collect::<Vec<(SgGateway, Vec<SgHttpRoute>)>>();
    Ok(config)
}
