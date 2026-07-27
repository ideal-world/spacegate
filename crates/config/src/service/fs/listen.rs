#[cfg(target_family = "unix")]
mod unix {
    use crate::service::fs::Fs;
    use crate::service::{ConfigEventType, ConfigType, CreateListener, Listen, ListenEvent, Retrieve};
    use crate::{model::Config, service::config_format::ConfigFormat, BoxError};
    use notify::{
        event::{EventKind, ModifyKind},
        Event, RecommendedWatcher, RecursiveMode, Watcher,
    };
    use spacegate_model::PluginInstanceId;
    use std::{collections::VecDeque, future::Future, path::Path, pin::Pin, task::Poll, time::Duration};

    /// Combines the files written by one configuration save into one consistent reload.
    const FILE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(300);

    /// Identifies a configuration object whose file can be reloaded independently.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum PendingConfigTarget {
        Plugin(PluginInstanceId),
        Route { gateway_name: String, name: String },
        Gateway(String),
    }

    /// Represents one classified filesystem change before a debounced batch is emitted.
    #[derive(Debug, Clone)]
    enum FileChange {
        Global,
        Target(PendingConfigTarget, ConfigEventType),
    }

    /// Accumulates one filesystem write batch and preserves dependency-safe event ordering.
    #[derive(Default)]
    struct PendingConfigEvents {
        global: bool,
        targets: Vec<(PendingConfigTarget, ConfigEventType)>,
    }

    impl PendingConfigEvents {
        /// Merges one filesystem change into the current batch.
        fn push(&mut self, change: FileChange) {
            match change {
                FileChange::Global => {
                    self.global = true;
                    self.targets.clear();
                }
                FileChange::Target(PendingConfigTarget::Plugin(_), ConfigEventType::Delete) => {
                    // Removing an instance can invalidate multiple routes, so retain the safe global reload path.
                    self.global = true;
                    self.targets.clear();
                }
                FileChange::Target(target, event_type) if !self.global => {
                    if let Some(index) = self.targets.iter().position(|(current, _)| current == &target) {
                        match merge_event_type(&self.targets[index].1, &event_type) {
                            Some(event_type) => self.targets[index].1 = event_type,
                            None => {
                                self.targets.remove(index);
                            }
                        }
                    } else {
                        self.targets.push((target, event_type));
                    }
                }
                FileChange::Target(_, _) => {}
            }
        }

        /// Builds listener events in plugin, route, then gateway order.
        fn into_events(self) -> VecDeque<ListenEvent> {
            if self.global {
                return VecDeque::from([(ConfigType::Global, ConfigEventType::Update).into()]);
            }
            let mut events = VecDeque::new();
            for priority in 0..=2 {
                for (target, event_type) in &self.targets {
                    if pending_target_priority(target) == priority {
                        events.push_back((target.to_config_type(), event_type.clone()).into());
                    }
                }
            }
            events
        }
    }

    impl PendingConfigTarget {
        /// Converts the internal filesystem target into the public configuration event target.
        fn to_config_type(&self) -> ConfigType {
            match self {
                Self::Plugin(id) => ConfigType::Plugin { id: id.clone() },
                Self::Route { gateway_name, name } => ConfigType::Route {
                    gateway_name: gateway_name.clone(),
                    name: name.clone(),
                },
                Self::Gateway(name) => ConfigType::Gateway { name: name.clone() },
            }
        }
    }

    /// Returns the stable dependency order for one pending configuration target.
    fn pending_target_priority(target: &PendingConfigTarget) -> u8 {
        match target {
            PendingConfigTarget::Plugin(_) => 0,
            PendingConfigTarget::Route { .. } => 1,
            PendingConfigTarget::Gateway(_) => 2,
        }
    }

    /// Merges repeated events for one file into the event type that represents its final state.
    fn merge_event_type(previous: &ConfigEventType, next: &ConfigEventType) -> Option<ConfigEventType> {
        match (previous, next) {
            (ConfigEventType::Create, ConfigEventType::Delete) => None,
            (ConfigEventType::Create, _) => Some(ConfigEventType::Create),
            (ConfigEventType::Delete, ConfigEventType::Create | ConfigEventType::Update) => Some(ConfigEventType::Update),
            (ConfigEventType::Delete, ConfigEventType::Delete) => Some(ConfigEventType::Delete),
            (ConfigEventType::Update, ConfigEventType::Delete) => Some(ConfigEventType::Delete),
            (ConfigEventType::Update, _) => Some(ConfigEventType::Update),
        }
    }

    pub struct FsListener {
        /// Keeps the recursive filesystem watcher alive for the listener lifetime.
        _watcher: RecommendedWatcher,
        /// Provides the existing manual full-reload escape hatch for operators.
        signal: tokio::signal::unix::Signal,
        /// Receives relevant filesystem changes from the synchronous notify callback.
        changes: tokio::sync::mpsc::UnboundedReceiver<FileChange>,
        /// Defers the reload until the current save batch has stopped producing changes.
        debounce: Option<Pin<Box<tokio::time::Sleep>>>,
        /// Stores classified changes until the current filesystem write batch is complete.
        pending: PendingConfigEvents,
        /// Holds dependency-ordered events that must be emitted one at a time.
        queued: VecDeque<ListenEvent>,
    }

    impl<F> CreateListener for Fs<F>
    where
        F: ConfigFormat + Clone + Send + Sync + 'static,
    {
        const CONFIG_LISTENER_NAME: &'static str = "file";
        type Listener = FsListener;
        async fn create_listener(&self) -> Result<(Config, Self::Listener), Box<dyn std::error::Error + Sync + Send + 'static>> {
            let config = self.retrieve_config().await?;
            Ok((config, FsListener::new(self.clone())?))
        }
    }

    impl FsListener {
        #[cfg(target_family = "unix")]
        pub fn new<F>(fs: Fs<F>) -> Result<Self, BoxError>
        where
            F: ConfigFormat + Clone + Send + 'static,
        {
            let (change_tx, changes) = tokio::sync::mpsc::unbounded_channel();
            let watch_dir = fs.dir.clone();
            let event_fs = fs.clone();
            let mut watcher = RecommendedWatcher::new(
                move |event: notify::Result<notify::Event>| {
                    let changes = match event {
                        Ok(event) => classify_event(&event_fs, &event),
                        Err(_) => vec![FileChange::Global],
                    };
                    for change in changes {
                        let _ = change_tx.send(change);
                    }
                },
                notify::Config::default(),
            )?;
            watcher.watch(&watch_dir, RecursiveMode::Recursive)?;
            let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
            Ok(Self {
                _watcher: watcher,
                signal,
                changes,
                debounce: None,
                pending: Default::default(),
                queued: Default::default(),
            })
        }
    }

    /// Classifies one notify event into configuration changes without reading partially written files.
    fn classify_event<F>(fs: &Fs<F>, event: &Event) -> Vec<FileChange>
    where
        F: ConfigFormat,
    {
        if matches!(event.kind, EventKind::Modify(ModifyKind::Name(_))) || event.paths.is_empty() {
            return vec![FileChange::Global];
        }
        let event_type = match event.kind {
            EventKind::Create(_) => ConfigEventType::Create,
            EventKind::Modify(_) => ConfigEventType::Update,
            EventKind::Remove(_) => ConfigEventType::Delete,
            _ => return Vec::new(),
        };
        let mut changes = Vec::new();
        for path in &event.paths {
            if path.exists() && path.is_dir() {
                continue;
            }
            match classify_config_path(fs, path) {
                ConfigPath::Global => return vec![FileChange::Global],
                ConfigPath::Target(target) => changes.push(FileChange::Target(target, event_type.clone())),
                ConfigPath::Unknown => return vec![FileChange::Global],
            }
        }
        changes
    }

    /// Describes whether one stable configuration file maps to an independent runtime object.
    enum ConfigPath {
        Global,
        Target(PendingConfigTarget),
        Unknown,
    }

    /// Maps an on-disk configuration file path to its runtime configuration scope.
    fn classify_config_path<F>(fs: &Fs<F>, path: &Path) -> ConfigPath
    where
        F: ConfigFormat,
    {
        if path == fs.entrance_config_path() {
            return ConfigPath::Global;
        }
        if path.parent() == Some(fs.plugin_dir().as_path()) {
            if path.extension() != Some(fs.format.extension()) {
                return ConfigPath::Unknown;
            }
            let Some(file_stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return ConfigPath::Unknown;
            };
            return ConfigPath::Target(PendingConfigTarget::Plugin(PluginInstanceId::from_file_stem(file_stem)));
        }
        if let Some((gateway_name, name)) = fs.extract_route_name(path) {
            return ConfigPath::Target(PendingConfigTarget::Route { gateway_name, name });
        }
        if path.parent() == Some(fs.gateway_dir().as_path()) && path.extension() == Some(fs.format.extension()) {
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return ConfigPath::Unknown;
            };
            return ConfigPath::Target(PendingConfigTarget::Gateway(name.to_string()));
        }
        let Ok(relative) = path.strip_prefix(fs.gateway_dir()) else {
            return ConfigPath::Unknown;
        };
        let mut components = relative.components();
        let Some(gateway_name) = components.next().and_then(|component| component.as_os_str().to_str()) else {
            return ConfigPath::Unknown;
        };
        let Some(file_name) = components.next().map(|component| component.as_os_str()) else {
            return ConfigPath::Unknown;
        };
        let expected_name = Path::new(crate::service::fs::MODULE_FILE_NAME).with_extension(fs.format.extension());
        if components.next().is_none() && file_name == expected_name.as_os_str() {
            ConfigPath::Target(PendingConfigTarget::Gateway(gateway_name.to_string()))
        } else {
            ConfigPath::Unknown
        }
    }

    impl Listen for FsListener {
        fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>> {
            if let Some(event) = self.queued.pop_front() {
                return Poll::Ready(Ok(event));
            }
            if let Poll::Ready(Some(_)) = self.signal.poll_recv(cx) {
                self.debounce = None;
                self.pending = Default::default();
                self.queued.clear();
                std::task::Poll::Ready(Ok((ConfigType::Global, ConfigEventType::Update).into()))
            } else {
                let mut changed = false;
                while let Poll::Ready(Some(change)) = self.changes.poll_recv(cx) {
                    self.pending.push(change);
                    changed = true;
                }
                if changed {
                    self.debounce = Some(Box::pin(tokio::time::sleep(FILE_CHANGE_DEBOUNCE)));
                }
                if let Some(debounce) = self.debounce.as_mut() {
                    if debounce.as_mut().poll(cx).is_ready() {
                        self.debounce = None;
                        self.queued = std::mem::take(&mut self.pending).into_events();
                        if let Some(event) = self.queued.pop_front() {
                            return Poll::Ready(Ok(event));
                        }
                    }
                }
                Poll::Pending
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::{
            future::poll_fn,
            path::PathBuf,
            time::{Duration, SystemTime, UNIX_EPOCH},
        };

        use tokio::time::timeout;

        use super::*;
        use crate::service::config_format::Json;
        use notify::event::CreateKind;

        fn test_config_dir() -> PathBuf {
            std::env::temp_dir().join(format!(
                "spacegate-fs-listener-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).expect("clock after unix epoch").as_nanos(),
            ))
        }

        #[test]
        fn emits_global_update_after_config_file_changes() {
            tokio::runtime::Builder::new_current_thread().enable_all().build().expect("create tokio runtime").block_on(async {
                let dir = test_config_dir();
                tokio::fs::create_dir_all(&dir).await.expect("create config directory");
                let fs = Fs::new(&dir, Json::default());
                let mut listener = FsListener::new(fs).expect("create file listener");

                tokio::fs::write(dir.join("config.json"), b"{}").await.expect("write config file");

                let event = timeout(Duration::from_secs(2), poll_fn(|cx| listener.poll_next(cx)))
                    .await
                    .expect("file write should trigger a configuration update")
                    .expect("listener should return an event");
                assert!(matches!(event.config, ConfigType::Global));
                assert!(matches!(event.r#type, ConfigEventType::Update));

                tokio::fs::remove_dir_all(dir).await.expect("remove test config directory");
            });
        }

        #[test]
        fn emits_route_update_after_route_file_changes() {
            tokio::runtime::Builder::new_current_thread().enable_all().build().expect("create tokio runtime").block_on(async {
                let dir = test_config_dir();
                let fs = Fs::new(&dir, Json::default());
                tokio::fs::create_dir_all(fs.routes_dir("gateway-a")).await.expect("create route directory");
                let mut listener = FsListener::new(fs.clone()).expect("create file listener");

                tokio::fs::write(fs.route_path("gateway-a", "route-a"), b"{}").await.expect("write route file");

                let event = timeout(Duration::from_secs(2), poll_fn(|cx| listener.poll_next(cx)))
                    .await
                    .expect("route write should trigger a configuration update")
                    .expect("listener should return an event");
                assert!(matches!(event.config, ConfigType::Route { gateway_name, name } if gateway_name == "gateway-a" && name == "route-a"));
                assert!(matches!(event.r#type, ConfigEventType::Create | ConfigEventType::Update));

                tokio::fs::remove_dir_all(dir).await.expect("remove test config directory");
            });
        }

        #[test]
        fn classifies_stable_config_paths_into_precise_events() {
            let dir = test_config_dir();
            std::fs::create_dir_all(&dir).expect("create config directory");
            let fs = Fs::new(&dir, Json::default());
            let plugin_path = fs.plugin_dir().join("wasm.authn.json");
            let route_path = fs.route_path("gateway-a", "route-a");
            let gateway_path = fs.gateway_main_config_path("gateway-a");
            for path in [&plugin_path, &route_path, &gateway_path] {
                std::fs::create_dir_all(path.parent().expect("config path parent")).expect("create config parent");
                std::fs::write(path, b"{}").expect("write config file");
            }

            let plugin = classify_event(&fs, &notify::Event::new(EventKind::Create(CreateKind::Any)).add_path(plugin_path));
            let route = classify_event(&fs, &notify::Event::new(EventKind::Create(CreateKind::Any)).add_path(route_path));
            let gateway = classify_event(&fs, &notify::Event::new(EventKind::Create(CreateKind::Any)).add_path(gateway_path));
            let global = classify_event(&fs, &notify::Event::new(EventKind::Create(CreateKind::Any)).add_path(fs.entrance_config_path()));

            assert!(matches!(plugin.as_slice(), [FileChange::Target(PendingConfigTarget::Plugin(_), ConfigEventType::Create)]));
            assert!(
                matches!(route.as_slice(), [FileChange::Target(PendingConfigTarget::Route { gateway_name, name }, ConfigEventType::Create)] if gateway_name == "gateway-a" && name == "route-a")
            );
            assert!(matches!(gateway.as_slice(), [FileChange::Target(PendingConfigTarget::Gateway(name), ConfigEventType::Create)] if name == "gateway-a"));
            assert!(matches!(global.as_slice(), [FileChange::Global]));

            std::fs::remove_dir_all(dir).expect("remove test config directory");
        }

        #[test]
        fn orders_precise_events_before_route_and_gateway_reloads() {
            let plugin = spacegate_model::PluginInstanceId::from_file_stem("wasm.authn");
            let mut pending = PendingConfigEvents::default();
            pending.push(FileChange::Target(PendingConfigTarget::Gateway("gateway-a".to_string()), ConfigEventType::Update));
            pending.push(FileChange::Target(
                PendingConfigTarget::Route {
                    gateway_name: "gateway-a".to_string(),
                    name: "route-a".to_string(),
                },
                ConfigEventType::Update,
            ));
            pending.push(FileChange::Target(PendingConfigTarget::Plugin(plugin.clone()), ConfigEventType::Create));

            let events = pending.into_events().into_iter().collect::<Vec<_>>();

            assert!(matches!(events.as_slice(), [
                ListenEvent { config: ConfigType::Plugin { id }, r#type: ConfigEventType::Create },
                ListenEvent { config: ConfigType::Route { gateway_name, name }, r#type: ConfigEventType::Update },
                ListenEvent { config: ConfigType::Gateway { name: gateway }, r#type: ConfigEventType::Update },
            ] if *id == plugin && gateway_name == "gateway-a" && name == "route-a" && gateway == "gateway-a"));
        }

        #[test]
        fn falls_back_to_global_reload_when_a_plugin_instance_is_deleted() {
            let plugin = spacegate_model::PluginInstanceId::from_file_stem("wasm.authn");
            let mut pending = PendingConfigEvents::default();
            pending.push(FileChange::Target(PendingConfigTarget::Plugin(plugin), ConfigEventType::Delete));

            let events = pending.into_events().into_iter().collect::<Vec<_>>();

            assert!(matches!(
                events.as_slice(),
                [ListenEvent {
                    config: ConfigType::Global,
                    r#type: ConfigEventType::Update,
                }]
            ));
        }
    }
}

#[cfg(target_family = "windows")]
mod windows {
    use std::task::ready;

    use notify::{Event, Watcher};

    use crate::service::fs::Fs;
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
}
