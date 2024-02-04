use std::future::Future;

use crate::{Config, ConfigItem};

mod fs;
mod k8s;


pub trait Retrieve {
    type Error: std::error::Error;
    fn retrieve_config_item(&self, name: &str) -> impl Future<Output = Result<Option<ConfigItem>, Self::Error>> + Send;
    fn retrieve_config(&self) -> impl Future<Output = Result<Config, Self::Error>> + Send;
}