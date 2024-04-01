use std::task::ready;

use notify::{Event, Watcher};

use super::Fs;
use crate::service::{ConfigEventType, ConfigType, CreateListener, Listen, ListenEvent, Retrieve};
use crate::{model::Config, service::config_format::ConfigFormat, BoxError};
pub struct FsListener {
    // hold the watcher, prevent dropping
    _watcher: notify::RecommendedWatcher,
    rx: tokio::sync::mpsc::UnboundedReceiver<(ConfigType, ConfigEventType)>,
}

impl<F> CreateListener for Fs<F>
where
    F: ConfigFormat + Clone + Send + Sync + 'static,
{
    const CONFIG_LISTENER_NAME: &'static str = "file";

    async fn create_listener(&self) -> Result<(Config, Box<dyn Listen>), Box<dyn std::error::Error + Sync + Send + 'static>> {
        let config = self.retrieve_config().await?;
        Ok((config, Box::new(FsListener::new(self.clone())?)))
    }
}

impl FsListener {
    pub fn new<F>(fs: Fs<F>) -> Result<Self, BoxError>
    where
        F: ConfigFormat + Clone + Send + 'static,
    {
        use notify::event::{AccessKind, AccessMode, EventKind};
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
                    if target == &fs.main_config_path() {
                        match evt.kind {
                            EventKind::Access(AccessKind::Close(AccessMode::Write)) => ConfigEventType::Update,
                            _ => {
                                return;
                            }
                        };
                    }

                    let _result = evt_tx.send((ConfigType::Global, ConfigEventType::Update));
                },
                Default::default(),
            )?
        };
        watcher.watch(&fs.dir, notify::RecursiveMode::Recursive)?;
        Ok(Self { _watcher: watcher, rx: evt_rx })
    }
}

impl Listen for FsListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>> {
        if let Some(next) = ready!(self.rx.poll_recv(cx)) {
            std::task::Poll::Ready(Ok(next))
        } else {
            std::task::Poll::Pending
        }
    }
}
