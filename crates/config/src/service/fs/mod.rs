use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
pub mod model;
use spacegate_model::{BoxError, BoxResult, Config, ConfigItem, PluginInstanceId, SgHttpRoute};
use tokio::sync::{Mutex, RwLock};

use crate::service::config_format::ConfigFormat;

pub const GATEWAY_DIR: &str = "gateway";
pub const ROUTE_DIR: &str = "route";
pub const PLUGIN_DIR: &str = "plugin";
pub const MODULE_FILE_NAME: &str = "config";
/// # Filesystem Configuration Backend
///
/// ## Structure
/// ``` no_rust
/// |- config.json
/// |- plugin/
/// |  |- plugin_code.json
/// |- gateway/
/// |  |- admin/
/// |  |  |- config.json
/// |  |  |- route/
/// |  |  |  |- static.json
/// |  |  |  |- api.json
/// |  |- app.json
///
/// ```
#[derive(Debug, Clone)]
pub struct Fs<F> {
    pub dir: Arc<Path>,
    pub format: F,
    pub buffer: Arc<Mutex<Vec<u8>>>,
    pub prev_retrieve_time: Arc<RwLock<SystemTime>>,
    pub cached: Arc<RwLock<Config>>,
    // /// None for always expire
    // pub cache_expire: Option<Duration>,
}

impl<F> Fs<F>
where
    F: ConfigFormat,
{
    pub fn entrance_config_path(&self) -> PathBuf {
        self.dir.join(MODULE_FILE_NAME).with_extension(self.format.extension())
    }

    pub async fn collect_config(&self) -> Result<Config, BoxError> {
        tracing::trace!("retrieve config");
        // main config file
        let mut config = {
            let main_config: Config = self.format.de(&tokio::fs::read(self.entrance_config_path()).await?)?;
            main_config
        };
        // collect plugin
        if let Ok(mut plugin_dir) = tokio::fs::read_dir(self.plugin_dir()).await.inspect_err(|e| tracing::debug!("fail to read plugin dir {e}")) {
            while let Some(entry) = plugin_dir.next_entry().await? {
                let Ok(file_name) = entry.file_name().into_string() else {
                    continue;
                };
                let plugin_id = PluginInstanceId::from_file_stem(&file_name);
                let spec: serde_json::Value = self.format.de(&tokio::fs::read(entry.path()).await?)?;
                config.plugins.insert(plugin_id, spec);
            }
        };
        // collect gateway
        {
            let dir_path = self.gateway_dir();
            let ext = self.format.extension();
            if let Ok(mut gateway_dir) = tokio::fs::read_dir(&dir_path).await.inspect_err(|e| {
                tracing::debug!("retrieve gateway dir error: {e}");
            }) {
                let mut gateway_names = HashSet::new();
                while let Some(entry) = gateway_dir.next_entry().await? {
                    let path = entry.path();
                    if (path.is_file() && path.extension() == Some(ext)) || path.is_dir() {
                        if let Some(gateway_name) = path.file_stem().and_then(OsStr::to_str) {
                            tracing::debug!("detected entry {gateway_name}");
                            gateway_names.insert(gateway_name.to_string());
                        }
                    }
                }
                for gateway_name in gateway_names {
                    if let Ok(Some(gateway)) = self.collect_gateway_item_config(&gateway_name).await.inspect_err(|e| tracing::debug!("fail to read gateway item: {e}")) {
                        config.gateways.insert(gateway_name, gateway);
                    }
                }
            }
        }
        tracing::trace!("config: {config:?}");
        Ok(config)
    }

    pub async fn save_config(&self, config: Config) -> Result<(), BoxError> {
        let Config { plugins, gateways, api_port } = config;
        let main_config_to_save: Config = Config { api_port, ..Default::default() };
        let b_main_config = self.format.ser(&main_config_to_save)?;
        tokio::fs::write(self.entrance_config_path(), &b_main_config).await?;
        for (id, spec) in plugins.into_inner().into_iter() {
            let path = self.plugin_path(&id);
            let b_spec = self.format.ser(&spec)?;
            tokio::fs::write(&path, &b_spec).await?;
        }
        for (gateway_name, item) in gateways.into_iter() {
            let dir = self.gateway_dir().join(&gateway_name);
            tokio::fs::create_dir_all(dir).await?;
            let gateway_path = self.gateway_main_config_path(&gateway_name);
            let b_gateway = self.format.ser(&item.gateway)?;
            tokio::fs::write(&gateway_path, &b_gateway).await?;
            let route_dir_path = self.routes_dir(&gateway_name);
            tokio::fs::create_dir_all(&route_dir_path).await?;
            for (route_name, route) in item.routes.into_iter() {
                let route_path = self.route_path(&gateway_name, &route_name);
                let b_route = self.format.ser(&route)?;
                tokio::fs::write(&route_path, &b_route).await?;
            }
        }
        Ok(())
    }

    pub async fn collect_gateway_item_config(&self, gateway_name: &str) -> Result<Option<ConfigItem>, BoxError> {
        let dir_path = self.gateway_dir();
        let ext = self.format.extension();
        // 1. retrieve <gateway_name>.<ext>
        let mut main_config_path = self.gateway_main_config_path(gateway_name);
        if !main_config_path.exists() {
            // 2. module config <gateway_name>/config.<ext>
            main_config_path = dir_path.join(gateway_name).with_extension(ext);
        }
        if !main_config_path.exists() {
            return Ok(None);
        }
        let mut main_config: ConfigItem = self.format.de(&tokio::fs::read(&main_config_path).await?)?;
        // 3. collect route config
        let route_dir_path = self.routes_dir(gateway_name);
        if route_dir_path.exists() {
            let mut route_dir = tokio::fs::read_dir(self.routes_dir(gateway_name)).await?;
            while let Some(entry) = route_dir.next_entry().await? {
                let path = entry.path();
                if path.is_file() && path.extension() == Some(ext) {
                    let Some(route_name) = path.file_stem().and_then(OsStr::to_str) else { continue };
                    if let Ok(route) = self.format.de::<SgHttpRoute>(&tokio::fs::read(&path).await?).inspect_err(|e| tracing::debug!("fail to read route config {path:?}: {e}")) {
                        main_config.routes.insert(route_name.to_string(), route);
                    }
                }
            }
        }
        Ok(Some(main_config))
    }

    pub async fn retrieve_cached<M, T>(&self, map: M) -> BoxResult<T>
    where
        M: FnOnce(&Config) -> T,
    {
        let config = self.collect_config().await?;
        let result = map(&config);
        Ok(result)
    }
    pub async fn modify_cached<M>(&self, map: M) -> BoxResult<()>
    where
        M: FnOnce(&mut Config) -> BoxResult<()>,
    {
        let mut config = self.collect_config().await?;
        let result = map(&mut config);
        if result.is_ok() {
            self.save_config(config).await?;
        }
        Ok(())
    }

    pub fn new<P: AsRef<Path>>(dir: P, format: F) -> Self {
        Self {
            buffer: Default::default(),
            dir: Arc::from(dir.as_ref().to_owned()),
            format,
            prev_retrieve_time: Arc::new(RwLock::new(SystemTime::UNIX_EPOCH)),
            cached: Default::default(), // cache: RwLock::new(None),
                                        // cache_expire: None,
        }
    }
    pub async fn clear_cache(&self) {
        *self.prev_retrieve_time.write().await = SystemTime::UNIX_EPOCH;
    }
    pub fn gateway_suffix(&self) -> OsString {
        let mut ext = OsString::from(GATEWAY_DIR);
        ext.push(OsStr::from_bytes(b"."));
        ext.push(self.format.extension());
        ext
    }

    pub fn gateway_dir(&self) -> PathBuf {
        self.dir.join(GATEWAY_DIR)
    }
    pub fn gateway_main_config_path(&self, gateway_name: &str) -> PathBuf {
        self.gateway_dir().join(gateway_name).join(MODULE_FILE_NAME).with_extension(self.format.extension())
    }

    pub fn routes_dir(&self, gateway_name: &str) -> PathBuf {
        self.gateway_dir().join(gateway_name).join(ROUTE_DIR)
    }

    pub fn route_path(&self, gateway_name: &str, route_name: &str) -> PathBuf {
        self.routes_dir(gateway_name).join(route_name).with_extension(self.format.extension())
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.dir.join(PLUGIN_DIR)
    }

    pub fn plugin_path(&self, id: &PluginInstanceId) -> PathBuf {
        let file_stem = id.as_file_stem();
        self.plugin_dir().join(file_stem).with_extension(self.format.extension())
    }
    pub fn extract_gateway_name_from_route_dir(&self, path: &Path) -> Option<String> {
        if path.extension()? == OsStr::from_bytes(ROUTE_DIR.as_bytes()) {
            path.file_stem().and_then(OsStr::to_str).map(|f| f.to_string())
        } else {
            None
        }
    }
    pub fn extract_route_name(&self, path: &Path) -> Option<(String, String)> {
        let gateway_name = self.extract_gateway_name_from_route_dir(path.parent()?)?;
        if path.extension()? == self.format.extension() {
            let route_name = path.file_stem().and_then(OsStr::to_str).map(|f| f.to_string())?;
            Some((gateway_name, route_name))
        } else {
            None
        }
    }
}

mod create;
mod delete;
mod discovery;
mod listen;
mod retrieve;
mod update;
