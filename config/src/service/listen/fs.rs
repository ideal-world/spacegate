use std::error::Error;

use notify::{Event, Watcher};

use crate::{
    service::{
        backend::fs::Fs,
        config_format::ConfigFormat,
        listen::{ConfigEventType, ConfigType},
    },
    BoxError,
};

use super::{CreateListener, Listen};
pub struct FsListener {
    // hold the watcher, prevent dropping
    _watcher: notify::RecommendedWatcher,
    rx: tokio::sync::mpsc::UnboundedReceiver<(ConfigType, ConfigEventType)>,
}

impl<F> CreateListener for Fs<F>
where
    F: ConfigFormat + Clone + Send + 'static,
{
    const CONFIG_LISTENER_NAME: &'static str = "file";

    fn create_listener(&self) -> Result<Box<dyn Listen>, Box<dyn Error + Sync + Send + 'static>> {
        Ok(Box::new(FsListener::new(self.clone())?))
    }
}

impl FsListener {
    pub fn new<F>(fs: Fs<F>) -> Result<Self, BoxError>
    where
        F: ConfigFormat + Clone + Send + 'static,
    {
        use notify::event::{AccessKind, AccessMode, CreateKind, EventKind, RemoveKind};
        let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = {
            let fs = fs.clone();
            notify::RecommendedWatcher::new(
                move |next| {
                    let evt: Event = match next {
                        Ok(evt) => evt,
                        Err(_e) => {
                            return;
                        }
                    };
                    let Some(target) = evt.paths.first() else {
                        // because we don't support rename or move or something else now
                        return;
                    };
                    // 1. path is a gateway file
                    let cfg_evt = if let Some(gateway_name) = fs.extract_gateway_name(target) {
                        let cfg_evt_ty = match evt.kind {
                            // 1.1 gateway added
                            EventKind::Create(CreateKind::File) => ConfigEventType::Create,
                            // 1.2. gateway modified
                            EventKind::Access(AccessKind::Close(AccessMode::Write)) => ConfigEventType::Update,
                            // 1.3. path is a gateway file and file is removed
                            EventKind::Remove(RemoveKind::File) => ConfigEventType::Delete,
                            // others, ignore
                            _ => {
                                return;
                            }
                        };
                        let cfg_ty = ConfigType::Gateway { name: gateway_name };
                        (cfg_ty, cfg_evt_ty)
                    }
                    // 2. path is a route file
                    else if let Some((gateway_name, route_name)) = fs.extract_route_name(target) {
                        let cfg_evt_ty = match evt.kind {
                            // 2.1 route added
                            EventKind::Create(CreateKind::File) => ConfigEventType::Create,
                            // 2.2 route modified
                            EventKind::Access(AccessKind::Close(AccessMode::Write)) => ConfigEventType::Update,
                            // 2.3. path is a route file and file is removed
                            EventKind::Remove(RemoveKind::File) => ConfigEventType::Delete,
                            // others, ignore
                            _ => {
                                return;
                            }
                        };
                        let cfg_ty = ConfigType::Route { gateway_name, name: route_name };
                        (cfg_ty, cfg_evt_ty)
                    }
                    // 3. path is a route directory
                    else if let Some(_gateway_name) = fs.extract_gateway_name_from_route_dir(target) {
                        // ignore route directory event
                        return;
                    } else {
                        return;
                    };
                    if evt_tx.send(cfg_evt).is_err() {}
                },
                Default::default(),
            )?
        };
        watcher.watch(&fs.dir, notify::RecursiveMode::Recursive)?;
        Ok(Self { _watcher: watcher, rx: evt_rx })
    }
}

impl Listen for FsListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<(super::ConfigType, super::ConfigEventType)>> {
        self.rx.poll_recv(cx)
    }
}
