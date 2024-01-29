use std::{
    path::{self, Path},
    sync::Arc,
    time::Duration,
};

use spacegate_tower::BoxError;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::Stream,
    log::{self, warn},
    tokio::{self, sync::Mutex, time},
    TardisFuns,
};

use crate::{do_startup, shutdown, update_route};

use super::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute, ConfigEvent, ConfigListener};
use lazy_static::lazy_static;

lazy_static! {
    static ref MD5_CACHE: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
}
use notify::{
    event::{AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind},
    Event, EventKind, INotifyWatcher, RecursiveMode, Watcher,
};
pub async fn init(conf_path: &str, check_interval_sec: u64) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    // let gateway_config_path = Arc::from(format!("{conf_path}/gateway.json"));
    // let routes_config_path = Arc::from(format!("{conf_path}/routes"));

    // let (config, _, _) = fetch_configs(&gateway_config_path, &routes_config_path).await?;
    // {
    //     let gateway_config_path = gateway_config_path.clone();
    //     let routes_config_path = routes_config_path.clone();
    //     let mut watcher = notify::recommended_watcher(move |res| {
    //         let event: Event = match res {
    //             Ok(event) => event,
    //             Err(e) => {
    //                 log::error!("[SG.Config.Local] notify error: {e}");
    //                 return;
    //             }
    //         };
    //         match event.kind {
    //             EventKind::Create(CreateKind::File) | EventKind::Modify(ModifyKind::Data(DataChange::Content)) | EventKind::Remove(RemoveKind::File) => {
    //                 let gateway_config_path = gateway_config_path.clone();
    //                 let routes_config_path = routes_config_path.clone();
    //                 tokio::spawn(async move {
    //                     log::trace!("[SG.Config] Config change check");
    //                     let (config, gateway_config_changed, routes_config_changed) =
    //                         fetch_configs(&gateway_config_path, &routes_config_path).await.expect("[SG.Config] init Failed to fetch configs");
    //                     if gateway_config_changed {
    //                         let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
    //                         shutdown(&gateway_config.name).await.expect("[SG.Config] shutdown failed");
    //                         do_startup(gateway_config, http_route_configs).await.expect("[SG.Config] re-startup failed");
    //                     } else if routes_config_changed {
    //                         let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
    //                         update_route(&gateway_config.name, http_route_configs).await.expect("[SG.Config] fail to update route config");
    //                     }
    //                 });
    //             }
    //             _ => return,
    //         }
    //     });
    // }
    // tardis::tokio::task::spawn_local(async move {
    //     let mut interval = time::interval(Duration::from_secs(check_interval_sec));
    //     loop {
    //         {
    //             log::trace!("[SG.Config] Config change check");
    //             let (config, gateway_config_changed, routes_config_changed) =
    //                 fetch_configs(&gateway_config_path, &routes_config_path).await.expect("[SG.Config] init Failed to fetch configs");
    //             if gateway_config_changed {
    //                 let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
    //                 shutdown(&gateway_config.name).await.expect("[SG.Config] shutdown failed");
    //                 do_startup(gateway_config, http_route_configs).await.expect("[SG.Config] re-startup failed");
    //             } else if routes_config_changed {
    //                 let (gateway_config, http_route_configs) = config.expect("[SG.Config] config is None");
    //                 update_route(&gateway_config.name, http_route_configs).await.expect("[SG.Config] fail to update route config");
    //             }
    //         }
    //         interval.tick().await;
    //     }
    // });
    // Ok(vec![config.expect("[SG.Config] config is None")])
    todo!()
}

async fn fetch_configs(gateway_config_path: &Path, routes_config_path: &Path) -> TardisResult<(Option<(SgGateway, Vec<SgHttpRoute>)>, bool, bool)> {
    let gateway_config_content = tokio::fs::read_to_string(&gateway_config_path).await?;
    if gateway_config_content.is_empty() {
        return Err(TardisError::not_found(&format!("[SG.Config] Gateway Config not found in {gateway_config_path:?} file"), ""));
    }
    let routes_config_content = {
        let mut routes_config_dir = tokio::fs::read_dir(&routes_config_path).await?;
        let mut routes_config_content = Vec::new();
        while let Some(route_config_dir) = routes_config_dir.next_entry().await? {
            routes_config_content.push(tokio::fs::read_to_string(&route_config_dir.path()).await?);
        }
        if routes_config_content.is_empty() {
            return Err(TardisError::not_found(
                &format!("[SG.Config] Routes Config not found in {routes_config_path:?} directory"),
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

pub struct FileConfigListener {
    pub conf_path: Arc<Path>,
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<ConfigEvent>,
    pub watcher: INotifyWatcher,
}

impl FileConfigListener {
    #[allow(clippy::collapsible_if)]
    pub async fn new(conf_path: impl AsRef<Path>, interval: Duration) -> Result<Self, BoxError> {
        let gateway_config_dir: Arc<Path> = conf_path.as_ref().join("gateway.json").into();
        let routes_config_path: Arc<Path> = conf_path.as_ref().join("routes").into();
        let conf_path: Arc<Path> = Arc::from(conf_path.as_ref().to_owned());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (config, _, _) = fetch_configs(&gateway_config_dir, &routes_config_path).await?;
        if let Some((gateway_config, http_route_configs)) = config {
            tx.send(ConfigEvent::GatewayAdd(gateway_config, http_route_configs))?;
        } else {
            warn!("[Sg.Config] Cannot find startup config");
        }
        let (notify_signal_tx, tokio_signal_rx) = std::sync::mpsc::channel::<()>();

        let reloader = async move {
            let gateway_config_path = gateway_config_dir.clone();
            let routes_config_path = routes_config_path.clone();
            loop {
                match tokio_signal_rx.try_recv() {
                    Ok(_) => {}
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        tokio::task::yield_now().await;
                    }
                    Err(_) => return,
                }
                log::trace!("[SG.Config] Config change check");
                if let Ok((Some((gateway_config, http_route_configs)), gateway_config_changed, routes_config_changed)) =
                    fetch_configs(&gateway_config_path, &routes_config_path).await
                {
                    if gateway_config_changed {
                        if tx.send(ConfigEvent::GatewayDeleteAll).is_err() {
                            return;
                        }
                        if tx.send(ConfigEvent::GatewayAdd(gateway_config, http_route_configs)).is_err() {
                            return;
                        }
                    } else if routes_config_changed {
                        if tx.send(ConfigEvent::HttpRouteReload(gateway_config.name, http_route_configs)).is_err() {
                            return;
                        }
                    }
                }
            }
        };
        tokio::task::spawn(reloader);
        // create watcher
        let watcher = {
            let mut watcher = notify::recommended_watcher(move |res| {
                let event: Event = match res {
                    Ok(event) => event,
                    Err(e) => {
                        log::error!("[SG.Config.Local] notify error: {e}");
                        return;
                    }
                };
                match event.kind {
                    EventKind::Access(AccessKind::Close(AccessMode::Write)) | EventKind::Remove(RemoveKind::File) => if notify_signal_tx.send(()).is_err() {},
                    _ => {}
                }
            })?;
            watcher.watch(path::Path::new(conf_path.as_ref()), RecursiveMode::Recursive)?;
            watcher
        };
        Ok(Self { conf_path, receiver: rx, watcher })
    }
}

impl Stream for FileConfigListener {
    type Item = ConfigEvent;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

impl ConfigListener for FileConfigListener {
    const CONFIG_LISTENER_NAME: &'static str = "file";

    fn shutdown(&mut self) {
        log::info!("[SG.Config] FileConfigListener shutdown")
        // match self.watcher.unwatch(self.conf_path.as_ref()) {
        //     Ok(_) => log::info!("[SG.Config] file config unwatch success"),
        //     Err(e) => log::error!("[SG.Config] file config unwatch failed: {e}"),
        // }
    }
}
