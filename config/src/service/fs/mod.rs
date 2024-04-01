use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
pub mod model;
use spacegate_model::{BoxResult, Config};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, RwLock},
};

use crate::service::config_format::ConfigFormat;

use self::model::{FsAsmPluginConfigMaybeUninitialized, MainFileConfig};

pub const GATEWAY_SUFFIX: &str = "gateway";
pub const ROUTES_SUFFIX: &str = "routes";

/// # Filesystem Configuration Backend
///
/// ## Structure
/// ``` no_rust
/// |- config.json
/// |- plugins/
/// |  |- plugin_code.plugin.json
/// |- gateway_name.gateway.json
/// |- gateway_name.routes/
/// |  |- route_name_1.json
/// |  |- route_name_2.json
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
    pub fn main_config_path(&self) -> PathBuf {
        self.dir.join("config").with_extension(self.format.extension())
    }
    pub async fn retrieve_cached<M, T>(&self, map: M) -> BoxResult<T>
    where
        M: FnOnce(&Config) -> T,
    {
        let path = self.main_config_path();
        let f_main_config = tokio::fs::OpenOptions::new().read(true).open(&path).await?;
        let mut g_buffer = self.buffer.lock().await;
        let mut prev_retrieve_time = self.prev_retrieve_time.write().await;
        let metadata = f_main_config.metadata().await?;
        let prev_modified_time = metadata.modified()?;
        if *prev_retrieve_time < prev_modified_time {
            let mut f_main_config = tokio::fs::OpenOptions::new().read(true).write(true).truncate(true).create(true).open(&path).await?;
            f_main_config.read_to_end(&mut g_buffer).await?;
            let config: MainFileConfig<FsAsmPluginConfigMaybeUninitialized> = self.format.de(&g_buffer).unwrap_or_default();
            let new_config = config.initialize_uid();
            let b_new_config = self.format.ser(&new_config)?;
            f_main_config.write_all(&b_new_config).await?;
            let mut new_model_config = new_config.into_model_config();
            let mut wg = self.cached.write().await;
            std::mem::swap(&mut new_model_config, &mut wg);
            *prev_retrieve_time = SystemTime::now();
            Ok((map)(&wg))
        } else {
            let rg = self.cached.read().await;
            Ok((map)(&rg))
        }
    }
    pub async fn modify_cached<M>(&self, map: M) -> BoxResult<()>
    where
        M: FnOnce(&mut Config) -> BoxResult<()>,
    {
        let mut f_main_config = tokio::fs::OpenOptions::new().read(true).write(true).truncate(true).create(true).open(self.main_config_path()).await?;
        let mut g_buffer = self.buffer.lock().await;
        let mut prev_retrieve_time = self.prev_retrieve_time.write().await;
        let metadata = f_main_config.metadata().await?;
        let prev_modified_time = metadata.modified()?;
        let mut wg = if *prev_retrieve_time < prev_modified_time {
            f_main_config.read_to_end(&mut g_buffer).await?;
            let config: MainFileConfig<FsAsmPluginConfigMaybeUninitialized> = self.format.de(&g_buffer).unwrap_or_default();
            let new_model_config = config.initialize_uid().into_model_config();
            let mut wg = self.cached.write().await;
            *wg = new_model_config;
            wg
        } else {
            self.cached.write().await
        };
        (map)(&mut wg)?;
        let new_file_config: MainFileConfig = wg.clone().into();
        let b_new_config = self.format.ser(&new_file_config)?;
        f_main_config.write_all(&b_new_config).await?;
        *prev_retrieve_time = SystemTime::now();
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
        let mut ext = OsString::from(GATEWAY_SUFFIX);
        ext.push(OsStr::from_bytes(b"."));
        ext.push(self.format.extension());
        ext
    }

    pub fn gateway_path(&self, name: &str) -> PathBuf {
        self.dir.join(name).with_extension(self.gateway_suffix())
    }

    pub fn routes_dir(&self, gateway_name: &str) -> PathBuf {
        self.dir.join(gateway_name).with_extension(ROUTES_SUFFIX)
    }

    pub fn route_path(&self, gateway_name: &str, route_name: &str) -> PathBuf {
        self.routes_dir(gateway_name).join(route_name).with_extension(self.format.extension())
    }

    pub fn extract_gateway_name(&self, path: &Path) -> Option<String> {
        let ext = self.gateway_suffix().into_string().expect("invalid gateway suffix");
        path.file_name().and_then(OsStr::to_str).and_then(|f| {
            if f.ends_with(&ext) {
                Some(f.trim_end_matches(&ext).trim_end_matches('.').to_string())
            } else {
                None
            }
        })
    }
    pub fn extract_gateway_name_from_route_dir(&self, path: &Path) -> Option<String> {
        if path.extension()? == OsStr::from_bytes(ROUTES_SUFFIX.as_bytes()) {
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
mod listen;
mod retrieve;
mod update;
