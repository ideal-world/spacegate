use std::{
    path::{self, Path},
    sync::Arc,
};

use spacegate_kernel::BoxError;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::Stream,
    log::{self, warn},
    tokio::{self, sync::Mutex},
    TardisFuns,
};

// use super::{ConfigEvent, ConfigListener};
use lazy_static::lazy_static;
use spacegate_config::{model::{http_route::SgHttpRoute, SgGateway}, service::{backend::fs::Fs, config_format::Json, retrieve::Retrieve}};

lazy_static! {
    static ref MD5_CACHE: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
}
use notify::{
    event::{AccessKind, AccessMode, RemoveKind},
    Event, EventKind, INotifyWatcher, RecursiveMode, Watcher,
};

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
    pub fs: Fs<Json>,
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<ConfigEvent>,
    pub watcher: INotifyWatcher,
}

impl FileConfigListener {
    #[allow(clippy::collapsible_if)]
    pub async fn new(conf_path: impl AsRef<Path>) -> Result<Self, BoxError> {
        let fs = Fs::new(conf_path, Json::default());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let config = fs.retrieve_config().await?;
        for (gateway_name, item) in config.gateways {
            tx.send(ConfigEvent::GatewayAdd(item.gateway, item.routes.into_values().collect()));
        }
        let (notify_signal_tx, tokio_signal_rx) = std::sync::mpsc::channel::<()>();

        let reloader = async move {
            let fs = fs.clone();
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
        Ok(Self { fs, receiver: rx, watcher })
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
        match self.watcher.unwatch(self.conf_path.as_ref()) {
            Ok(_) => log::info!("[SG.Config] file config unwatch success"),
            Err(e) => log::error!("[SG.Config] file config unwatch failed: {e}"),
        }
    }
}
