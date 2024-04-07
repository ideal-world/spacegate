use std::collections::VecDeque;

use futures_util::{Stream, StreamExt};
use spacegate_config::service::{ConfigEventType, ConfigType, CreateListener, Listen, Retrieve};

use spacegate_plugin::SgPluginRepository;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

// pub mod config_by_k8s;
// pub mod config_by_local;
// pub mod config_by_redis;
pub use spacegate_config::model::*;
use tracing::info;

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
        #[cfg(feature = "ext-axum")]
        {
            use spacegate_ext_axum::axum;
            info!("Starting web server...");
            let cancel_token = shutdown_signal.clone();
            let server = spacegate_ext_axum::GlobalAxumServer::default();
            server
                .modify_router(|router| {
                    router.fallback(axum::routing::any(axum::response::Html(axum::body::Bytes::from_static(include_bytes!(
                        "./config/web-server-index.html"
                    )))))
                })
                .await;
            spacegate_plugin::ext::axum::register_plugin_routes().await;
            server.set_cancellation(cancel_token).await;
            server.start().await?;
            info!("Web server started.");
        }
        let (init_config, listener) = config.create_listener().await?;
        RunningSgGateway::global_init(init_config, shutdown_signal.clone()).await;
        let mut listener = ListenerWrapper(listener);
        tracing::info!("[SG.Config] Entering listening");
        let mut local_queue = VecDeque::new();
        loop {
            let event = if let Some(next) = local_queue.pop_front() {
                next
            } else {
                tokio::select! {
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
                (ConfigType::Plugin { id }, ConfigEventType::Create | ConfigEventType::Update) => {
                    let config = config.retrieve_plugin(&id).await?;
                    if let Some(config) = config {
                        if let Err(e) = SgPluginRepository::global().create_or_update_instance(config) {
                            tracing::error!("[SG.Config] plugin {id:?} create failed: {e}", id = id, e = e);
                        }
                    } else {
                        tracing::error!("[SG.Config] plugin {id:?} not found");
                    }
                }
                (ConfigType::Plugin { id }, ConfigEventType::Delete) => match SgPluginRepository::global().remove_instance(&id) {
                    Ok(_mount_points) => {
                        // TODO: remove mount points
                    }
                    Err(e) => {
                        tracing::error!("[SG.Config] file to remove plugin {id:?} : {e}", id = id, e = e);
                    }
                },
                (ConfigType::Global, _) => {
                    let config = config.retrieve_config().await?;
                    RunningSgGateway::global_reset().await;
                    RunningSgGateway::global_init(config, shutdown_signal.clone()).await;
                }
            }
        }
    })
}
