use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt::{self, Debug, Formatter},
    net::{IpAddr, Ipv6Addr},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    str::FromStr,
};

use clap::Parser;
use serde_json::Value;
use tracing::{info, warn};

use crate::state::PluginCode;
const DEFAULT_HOST: IpAddr = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
const DEFAULT_PORT: u16 = 80;
/// Arguments to initiate the server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    ///
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    ///
    #[arg(short='H', long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,
    /// the config backend you choose

    /// see [`ConfigBackend`]
    #[arg(short, long, default_value_t = ConfigBackend::File(PathBuf::from("./config")))]
    pub config: ConfigBackend,
    /// the plugin schemas
    ///
    /// see [`Schemas`]
    #[arg(short, long, default_value_t = Schemas(PathBuf::from("./schema")))]
    pub schemas: Schemas,
}

#[derive(Debug, Clone)]
pub struct Schemas(PathBuf);

impl Schemas {
    pub fn load_all(&self) -> Result<HashMap<PluginCode, Value>, Box<dyn std::error::Error>> {
        let mut map = HashMap::new();
        let Ok(dir) = std::fs::read_dir(&self.0) else {
            warn!("cannot read schema dir {:?}", self.0);
            return Ok(map);
        };
        for entry in dir.flatten() {
            if entry.path().is_file() && entry.path().extension() == Some(OsStr::from_bytes(b"json")) {
                if let Some(plugin_name) = entry.path().file_stem().and_then(OsStr::to_str) {
                    let bin = std::fs::read(entry.path())?;
                    let _ = serde_json::from_slice(&bin).map(|v| map.insert(PluginCode::plugin(plugin_name), v)).inspect_err(|e| warn!("invalid schema: {e}"));
                }
            }
        }
        info!("all schema loaded");
        Ok(map)
    }
}

impl FromStr for Schemas {
    type Err = <std::path::PathBuf as std::str::FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        std::path::PathBuf::from_str(s).map(Schemas)
    }
}

impl fmt::Display for Schemas {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone)]
pub enum ConfigBackend {
    /// File backend
    ///
    /// example: file:/path/to/file
    File(PathBuf),
    K8s(String),
}

impl FromStr for ConfigBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((kind, resource)) = s.split_once(':') {
            match kind {
                "file" => Ok(ConfigBackend::File(PathBuf::from(resource))),
                "k8s" => Ok(ConfigBackend::K8s(resource.to_string())),
                _ => Err(format!("unknown backend kind: {}", kind)),
            }
        } else {
            Err("missing backend kind".to_string())
        }
    }
}

impl fmt::Display for ConfigBackend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigBackend::File(path) => write!(f, "file:{}", path.display()),
            ConfigBackend::K8s(ns) => write!(f, "k8s:{}", ns),
        }
    }
}
