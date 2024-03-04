use std::error::Error;

use futures_util::Future;

use crate::{BoxError, Config};

mod fs;
#[cfg(feature = "k8s")]
mod k8s;
mod memory;
#[cfg(feature = "redis")]
mod redis;

pub enum ConfigEventType {
    Create,
    Update,
    Delete,
}

pub enum ConfigType {
    Gateway { name: String },
    Route { gateway_name: String, name: String },
}

pub trait CreateListener {
    const CONFIG_LISTENER_NAME: &'static str;
    fn create_listener(&self) -> impl Future<Output = Result<(Config, Box<dyn Listen>), Box<dyn Error + Sync + Send + 'static>>> + Send;
}

pub type ListenEvent = (ConfigType, ConfigEventType);
pub trait Listen: Unpin {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<ListenEvent, BoxError>>;
}
