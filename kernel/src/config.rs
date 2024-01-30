use serde::{Deserialize, Serialize};
use spacegate_tower::BoxError;
use tardis::{
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
                            log::info!("[SG.Config] config event stream end");
                            log::info!("[SG.Config] config listener {CONFIG_LISTENER_NAME} shutdown", CONFIG_LISTENER_NAME = L::CONFIG_LISTENER_NAME);
                            config_listener.shutdown();
                            return Ok(());
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
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("[SG.Config] Fail to reload routes: {e}")
                        }
                    }
                }
            }
        }
    })
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct StaticConfigItem {
    pub gateway: SgGateway,
    pub routes: Vec<SgHttpRoute>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]

pub struct StaticConfig {
    pub items: Vec<StaticConfigItem>,
}

impl Stream for StaticConfig {
    type Item = ConfigEvent;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        if self.items.is_empty() {
            return std::task::Poll::Ready(None);
        }
        let item = self.items.remove(0);
        let routes = item.routes;
        std::task::Poll::Ready(Some(ConfigEvent::GatewayAdd(item.gateway, routes)))
    }
}

impl ConfigListener for StaticConfig {
    const CONFIG_LISTENER_NAME: &'static str = "static";
    fn shutdown(&mut self) {}
}
