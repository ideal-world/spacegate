use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::OnceLock,
};

use clap::Parser;
const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const DEFAULT_PORT: u16 = 10808;
const DEFAULT_CERT_PATH: &str = ".crt";
const DEFAULT_KEY_PATH: &str = ".key";
#[derive(Debug, Parser, Clone)]
pub struct CommandArgs {
    #[arg(short, long, env, default_value = DEFAULT_CERT_PATH)]
    pub cert: PathBuf,

    #[arg(short, long, env, default_value = DEFAULT_KEY_PATH)]
    pub key: PathBuf,

    #[arg(short = 'H', long, env, default_value_t = DEFAULT_HOST)]
    pub host: IpAddr,

    #[arg(short, long, env, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}

impl CommandArgs {
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn args() -> &'static CommandArgs {
    static ARGS: OnceLock<CommandArgs> = OnceLock::new();
    ARGS.get_or_init(CommandArgs::parse)
}
