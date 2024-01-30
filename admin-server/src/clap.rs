use std::{
    fmt::{self, Formatter}, net::{IpAddr, Ipv6Addr}, path::PathBuf, str::FromStr
};

use clap::Parser;
const DEFAULT_HOST: IpAddr = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
const DEFAULT_PORT: u16 = 80;
/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    #[arg(long, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,
    #[arg(short, long, default_value_t = Backend::File(PathBuf::from("./")))]
    pub backend: Backend,
}

#[derive(Debug, Clone)]
pub enum Backend {
    /// File backend
    /// 
    /// example: file:/path/to/file
    File(PathBuf),
}



impl FromStr for Backend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((kind, resource)) = s.split_once(':') {
            match kind {
                "file" => Ok(Backend::File(PathBuf::from(resource))),
                _ => Err(format!("unknown backend kind: {}", kind)),
            }
        } else {
            Err("missing backend kind".to_string())
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::File(path) => write!(f, "file:{}", path.display()),
        }
    }
}
