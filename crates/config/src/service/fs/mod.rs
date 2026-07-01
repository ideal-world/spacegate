use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
pub mod model;
use spacegate_model::{BoxError, BoxResult, Config, ConfigItem, PluginInstanceId};
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
        let ext = self.format.extension();
        if let Ok(mut plugin_dir) = tokio::fs::read_dir(self.plugin_dir()).await.inspect_err(|e| tracing::debug!("fail to read plugin dir {e}")) {
            while let Some(entry) = plugin_dir.next_entry().await? {
                let path = entry.path();
                if !path.is_file() || path.extension() != Some(ext) {
                    continue;
                };
                let Some(file_stem) = path.file_stem().and_then(OsStr::to_str) else {
                    continue;
                };
                let plugin_id = PluginInstanceId::from_file_stem(file_stem);
                let spec: serde_json::Value = self.format.de(&tokio::fs::read(path).await?)?;
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
        // save config
        let Config {
            plugins,
            gateways,
            api_port,
            observability,
        } = config;
        let main_config_to_save: Config = Config {
            api_port,
            observability,
            ..Default::default()
        };
        let b_main_config = self.format.ser(&main_config_to_save)?;
        tokio::fs::write(self.entrance_config_path(), &b_main_config).await?;
        if !plugins.is_empty() {
            tokio::fs::create_dir_all(self.plugin_dir()).await?;
            for (id, spec) in plugins.into_inner().into_iter() {
                let path = self.plugin_path(&id);
                let b_spec = self.format.ser(&spec)?;
                tokio::fs::write(&path, &b_spec).await?;
            }
        }
        for (gateway_name, item) in gateways.into_iter() {
            let dir = self.gateway_dir().join(&gateway_name);
            tokio::fs::create_dir_all(dir).await?;
            let gateway_path = self.gateway_main_config_path(&gateway_name);
            let b_gateway = self.format.ser(&ConfigItem {
                gateway: item.gateway,
                ..Default::default()
            })?;
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
                    if let Ok(route) =
                        self.format.de::<spacegate_model::SgRoute>(&tokio::fs::read(&path).await?).inspect_err(|e| tracing::debug!("fail to read route config {path:?}: {e}"))
                    {
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
        map(&mut config)?;
        self.clear_config_dir().await?;
        self.save_config(config).await?;
        Ok(())
    }

    async fn clear_config_dir(&self) -> BoxResult<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                tokio::fs::remove_dir_all(path).await?;
            } else {
                tokio::fs::remove_file(path).await?;
            }
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
        let file_name = format!("{}.{}", id.as_file_stem(), self.format.extension().to_string_lossy());
        self.plugin_dir().join(file_name)
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

#[cfg(test)]
mod tests {
    use super::Fs;
    use crate::service::{config_format::Json, Create, Delete, Retrieve};
    use serde_json::json;
    use spacegate_model::{Config, PluginInstanceId, PluginInstanceName};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_config_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("spacegate-config-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), serde_json::to_vec(&Config::default()).unwrap()).unwrap();
        dir
    }

    fn named_wasm(name: &str) -> PluginInstanceId {
        PluginInstanceId {
            code: "wasm".into(),
            name: PluginInstanceName::Named { name: name.to_string() },
        }
    }

    #[test]
    fn fs_named_plugin_file_keeps_full_stem() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let dir = temp_config_dir("named-plugin");
            let fs_backend = Fs::new(&dir, Json::default());
            let id = named_wasm("third-party-authn");

            fs_backend.create_plugin(&id, json!({ "plugin_name": "authn" })).await.unwrap();

            assert!(dir.join("plugin/wasm.third-party-authn.json").exists());
            assert!(!dir.join("plugin/wasm.json").exists());
            let got = fs_backend.retrieve_plugin(&id).await.unwrap().unwrap();
            assert_eq!(got.id, id);
            assert_eq!(got.spec["plugin_name"], "authn");

            fs::remove_dir_all(dir).unwrap();
        });
    }

    #[test]
    fn fs_delete_plugin_only_removes_target_plugin_file() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let dir = temp_config_dir("delete-plugin");
            let fs_backend = Fs::new(&dir, Json::default());
            let first = named_wasm("first");
            let second = named_wasm("second");

            fs_backend.create_plugin(&first, json!({ "plugin_name": "first" })).await.unwrap();
            fs_backend.create_plugin(&second, json!({ "plugin_name": "second" })).await.unwrap();
            fs_backend.delete_plugin(&first).await.unwrap();

            assert!(!dir.join("plugin/wasm.first.json").exists());
            assert!(dir.join("plugin/wasm.second.json").exists());
            assert!(dir.join("config.json").exists());
            assert!(fs_backend.retrieve_plugin(&second).await.unwrap().is_some());

            fs::remove_dir_all(dir).unwrap();
        });
    }
}
