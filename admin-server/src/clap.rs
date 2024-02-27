use std::{
    fmt::{self, Formatter},
    net::{IpAddr, Ipv6Addr},
    path::PathBuf,
    str::FromStr,
};

use clap::Parser;
const DEFAULT_HOST: IpAddr = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
const DEFAULT_PORT: u16 = 80;
/// Arguments to initiate the server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    #[arg(short='H', long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,
    /// the config backend you choose

    /// see [`ConfigBackend`]
    #[arg(short, long, default_value_t = ConfigBackend::File(PathBuf::from("./")))]
    pub config: ConfigBackend,
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
