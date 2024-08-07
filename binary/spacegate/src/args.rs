use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone)]
pub enum Config {
    #[cfg(feature = "fs")]
    File(PathBuf),
    #[cfg(feature = "k8s")]
    K8s(String),
    #[cfg(feature = "redis")]
    Redis(String),
    Static(PathBuf),
}

impl FromStr for Config {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((kind, resource)) = s.split_once(':') {
            match kind {
                "file" => {
                    #[cfg(feature = "fs")]
                    {
                        Ok(Config::File(PathBuf::from(resource)))
                    }
                    #[cfg(not(feature = "fs"))]
                    {
                        Err(format!("config backend kind {} not enabled, please select a correct build", kind))
                    }
                }
                #[cfg(feature = "k8s")]
                "k8s" => {
                    {
                        Ok(Config::K8s(resource.to_string()))
                    }
                    #[cfg(not(feature = "fs"))]
                    {
                        Err(format!("config backend kind {} not enabled, please select a correct build", kind))
                    }
                }
                #[cfg(feature = "redis")]
                "redis" => {
                    {
                        Ok(Config::Redis(resource.to_string()))
                    }
                    #[cfg(not(feature = "fs"))]
                    {
                        Err(format!("config backend kind {} not enabled, please select a correct build", kind))
                    }
                }
                "static" => Ok(Config::Static(PathBuf::from(resource))),
                _ => Err(format!("unknown config backend kind: {}", kind)),
            }
        } else {
            Err("missing config backend kind".to_string())
        }
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "fs")]
            Config::File(path) => write!(f, "file:{}", path.display()),
            #[cfg(feature = "k8s")]
            Config::K8s(ns) => write!(f, "k8s:{}", ns),
            #[cfg(feature = "redis")]
            Config::Redis(url) => write!(f, "redis:{}", url),
            Config::Static(path) => write!(f, "static:{}", path.to_string_lossy()),
        }
    }
}

impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Config, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Config::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Spacegate start up arguments
#[derive(Debug, Serialize, Deserialize, Clone, Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// The config file path
    ///
    /// # Example
    /// ## File
    /// `-c file:/path/to/dir`
    /// ## K8s
    /// `-c k8s:namespace`
    /// ## Redis
    /// `-c redis:redis://some-redis-url`
    #[arg(short, long, env)]
    #[cfg_attr(feature = "build-k8s", arg(default_value_t=Config::K8s(String::from("default"))))]
    #[cfg_attr(all(not(feature = "build-k8s"), target_family = "unix", feature="fs"), arg(default_value_t=Config::File(PathBuf::from("/etc/spacegate"))))]
    pub config: Config,
    /// The dynamic lib plugins dir
    ///
    /// # Example
    /// If you are using linux, you may put the plugins dll in `/lib/spacegate/plugins`.
    /// `-p /lib/spacegate/plugins`
    #[arg(short, long, env)]
    #[cfg_attr(target_family = "unix", arg(default_value = "/lib/spacegate/plugins"))]
    pub plugins: Option<PathBuf>,
}
