use futures_util::{Stream, StreamExt};
use spacegate_config::service::{ConfigEventType, ConfigType, CreateListener, Listen, Retrieve};
use spacegate_kernel::BoxError;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

// pub mod config_by_k8s;
// pub mod config_by_local;
// pub mod config_by_redis;
pub use spacegate_config::model::*;

pub(crate) mod matches_convert;
pub mod plugin_filter_dto;

pub struct ListenerWrapper(Box<dyn Listen>);

impl Stream for ListenerWrapper {
    type Item = (ConfigType, ConfigEventType);

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_next(cx).map_err(|e| tracing::error!("[SG.Config] listening gateway error: {e}")).map(Result::ok)
    }
}

pub fn init_with_config<C>(config: C, shutdown_signal: CancellationToken) -> JoinHandle<Result<(), BoxError>>
where
    C: Retrieve + CreateListener + 'static,
{
    use crate::server::RunningSgGateway;
    tokio::task::spawn_local(async move {
        let (init_config, listener) = config.create_listener().await?;
        for (name, item) in init_config.gateways {
            match RunningSgGateway::create(item, shutdown_signal.clone()) {
                Ok(inst) => RunningSgGateway::global_save(name, inst),
                Err(e) => {
                    tracing::error!("[SG.Config] fail to init gateway [{name}]: {e}")
                }
            }
        }
        let mut listener = ListenerWrapper(listener);
        tracing::info!("[SG.Config] Entering listening");
        loop {
            let event = tokio::select! {
                _ = shutdown_signal.cancelled() => {
                    tracing::info!("[SG.Config] config listener {CONFIG_LISTENER_NAME} shutdown", CONFIG_LISTENER_NAME = C::CONFIG_LISTENER_NAME);
                    // listener.shutdown();
                    return Ok(());
                }
                event = listener.next() => {
                    match event {
                        Some(event) => event,
                        None => {
                            tracing::info!("[SG.Config] config event stream end");
                            tracing::info!("[SG.Config] config listener {CONFIG_LISTENER_NAME} shutdown", CONFIG_LISTENER_NAME = C::CONFIG_LISTENER_NAME);
                            // config.shutdown();
                            return Ok(());
                        }
                    }
                }
            };
            match event {
                (ConfigType::Gateway { name }, ConfigEventType::Create) => {
                    if let Some(config) = config.retrieve_config_item(&name).await? {
                        tracing::info!("[SG.Config] gateway {name} created", name = name);
                        if let Ok(gateway) = RunningSgGateway::create(config, shutdown_signal.clone()) {
                            RunningSgGateway::global_save(name, gateway);
                        }
                    }
                }
                (ConfigType::Gateway { name }, ConfigEventType::Update) => {
                    if let Some(config) = config.retrieve_config_item(&name).await? {
                        tracing::info!("[SG.Config] gateway {name} updated", name = name);
                        if let Some(inst) = RunningSgGateway::global_remove(&name) {
                            inst.shutdown().await;
                        }
                        if let Ok(gateway) = RunningSgGateway::create(config, shutdown_signal.clone()) {
                            RunningSgGateway::global_save(name, gateway);
                        }
                    }
                }
                (ConfigType::Gateway { name }, ConfigEventType::Delete) => {
                    tracing::info!("[SG.Config] gateway {name} deleted", name = name);
                    if let Some(inst) = RunningSgGateway::global_remove(name) {
                        inst.shutdown().await;
                    }
                }
                (ConfigType::Route { gateway_name, name }, _) => {
                    let routes = config.retrieve_config_item_all_routes(&gateway_name).await?;
                    tracing::info!("[SG.Config] route {name} modified", name = name);
                    if let Err(e) = RunningSgGateway::global_update(&gateway_name, routes).await {
                        tracing::error!("[SG.Config] route {name} modified failed: {e}", name = name, e = e);
                    }
                }
            }
        }
    })
}
