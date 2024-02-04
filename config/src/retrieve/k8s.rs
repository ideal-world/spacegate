use crate::{backend::k8s::K8s, Config, ConfigItem};
use super::Retrieve;

impl Retrieve for K8s  {
    type Error = kube::Error;

    async fn retrieve_config_item(&self, name: &str) -> Result<Option<ConfigItem>, Self::Error> {
        todo!()
    }

    async fn retrieve_config(&self) -> Result<Config, Self::Error> {
        todo!()
    }
}