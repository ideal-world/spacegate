use std::future::Future;

use spacegate_tower::BoxError;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::{Stream, StreamExt},
    log,
    tokio::{self, task::JoinHandle},
};
use tokio_util::sync::CancellationToken;

use crate::config::gateway_dto::SgGateway;

use self::http_route_dto::SgHttpRoute;

#[cfg(feature = "k8s")]
pub mod config_by_k8s;
#[cfg(feature = "local")]
pub mod config_by_local;
#[cfg(feature = "cache")]
pub mod config_by_redis;
pub mod gateway_dto;
pub mod http_route_dto;
#[cfg(feature = "k8s")]
pub mod k8s_crd;
#[cfg(feature = "k8s")]
mod k8s_crd_spaceroute;
pub mod plugin_filter_dto;

// #[allow(unreachable_code)]
// #[allow(unused_variables)]
// pub async fn init(k8s_mode: bool, namespace_or_conf_uri: Option<String>, check_interval_sec: Option<u64>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
//     log::info!(
//         "[SG.Config] Config initialization mode: {}",
//         if k8s_mode {
//             format!("kubernetes, with namespaces: {namespace_or_conf_uri:?}")
//         } else {
//             format!("non-kubernetes, with uri: {namespace_or_conf_uri:?}")
//         }
//     );
//     if k8s_mode {
//         #[cfg(feature = "k8s")]
//         {
//             config_by_k8s::init(namespace_or_conf_uri).await
//         }
//         #[cfg(not(feature = "k8s"))]
//         {
//             Err(tardis::basic::error::TardisError::not_found(
//                 "[SG.Config] The current compilation mode does not support k8s",
//                 "",
//             ))
//         }
//     } else {
//         let conf_uri = namespace_or_conf_uri.ok_or_else(|| TardisError::not_found("[SG.Config] The configuration path must be specified in the current mode", ""))?;
//         #[cfg(feature = "cache")]
//         {
//             return config_by_redis::init(&conf_uri, check_interval_sec.unwrap_or(10)).await;
//         }
//         #[cfg(feature = "local")]
//         {
//             return config_by_local::init(&conf_uri, check_interval_sec.unwrap_or(10)).await;
//         }
//         Err(tardis::basic::error::TardisError::not_found("[SG.Config] The current compilation mode does not exist", ""))
//     }
// }

pub enum ConfigEvent {
    GatewayAdd(SgGateway, Vec<SgHttpRoute>),
    GatewayDelete(String),
    GatewayDeleteAll,
    HttpRouteReload(String, Vec<SgHttpRoute>),
}

pub trait ConfigListener: Stream<Item = ConfigEvent> + Send + Sync + Unpin + 'static {
    const CONFIG_LISTENER_NAME: &'static str;
    fn shutdown(&mut self);
}

pub fn init_with_config_listener<L>(mut config_listener: L, shutdown_signal: CancellationToken) -> JoinHandle<Result<(), BoxError>>
where
    L: ConfigListener,
{
    use crate::server::RunningSgGateway;

    tardis::tokio::task::spawn_local(async move {
        loop {
            let event = tokio::select! {
                _ = shutdown_signal.cancelled() => {
                    log::info!("[SG.Config] config listener {CONFIG_LISTENER_NAME} shutdown", CONFIG_LISTENER_NAME = L::CONFIG_LISTENER_NAME);
                    config_listener.shutdown();
                    return Ok(());
                }
                event = config_listener.next() => {
                    match event {
                        Some(event) => event,
                        None => {
                            log::error!("[SG.Config] unexpected end of event stream config");
                            log::info!("[SG.Config] config listener {CONFIG_LISTENER_NAME} shutdown", CONFIG_LISTENER_NAME = L::CONFIG_LISTENER_NAME);
                            config_listener.shutdown();
                            return Err(BoxError::from("[SG.Config] unexpected end of event stream config"));
                        }
                    }
                }
            };
            match event {
                ConfigEvent::GatewayAdd(gateway, http_routes) => {
                    let gateway_name = gateway.name.clone();
                    if let Some(prev_instance) = RunningSgGateway::global_remove(&gateway_name) {
                        log::info!(
                            "[SG.Config] GatewayReload: {gateway_name} with {routes_count} routes",
                            gateway_name = gateway.name,
                            routes_count = http_routes.len()
                        );
                        prev_instance.shutdown().await;
                    } else {
                        log::info!(
                            "[SG.Config] GatewayAdd: {gateway_name} with {routes_count} routes",
                            gateway_name = gateway.name,
                            routes_count = http_routes.len()
                        );
                    }
                    match RunningSgGateway::create(gateway, http_routes, shutdown_signal.child_token()) {
                        Ok(new_gateway) => {
                            RunningSgGateway::global_save(gateway_name, new_gateway);
                        }
                        Err(e) => {
                            log::error!("[SG.Config] Fail to create gateway: {e}")
                        }
                    }
                }
                ConfigEvent::GatewayDelete(gateway_name) => {
                    log::info!("[SG.Config] GatewayDelete: {gateway_name}", gateway_name = gateway_name);
                    if let Some(prev_instance) = RunningSgGateway::global_remove(&gateway_name) {
                        prev_instance.shutdown().await;
                    }
                }
                ConfigEvent::GatewayDeleteAll => {
                    log::info!("[SG.Config] GatewayDeleteAll");
                    let instances = RunningSgGateway::global_store().lock().expect("fail to lock").drain().collect::<Vec<_>>();
                    for (_, inst) in instances {
                        inst.shutdown().await;
                    }
                }
                ConfigEvent::HttpRouteReload(gateway_name, http_routes) => {
                    log::info!("[SG.Config] HttpRouteReload: {gateway_name}", gateway_name = gateway_name);
                    match RunningSgGateway::global_update(&gateway_name, http_routes).await {
                        Ok(_) => {},
                        Err(e) => {
                            log::error!("[SG.Config] Fail to reload routes: {e}")
                        },
                    }
                }
            }
        }
    })
}
