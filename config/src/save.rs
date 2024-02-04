use std::future::Future;

use crate::{Config, ConfigItem};

pub mod fs;

pub trait Save {
    type Error: std::error::Error;
    fn save_config_item(&self, name: &str, item: &ConfigItem) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn save_config(&self, config: &Config) -> impl Future<Output = Result<Config, Self::Error>> + Send;
}
