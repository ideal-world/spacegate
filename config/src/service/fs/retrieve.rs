use std::ffi::OsStr;

use spacegate_model::{PluginConfig, PluginInstanceId, PluginInstanceMap};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;

use super::{Fs, GATEWAY_SUFFIX};
use crate::service::config_format::ConfigFormat;
use crate::service::fs::model::{FsAsmPluginConfigMaybeUninitialized, MainFileConfig};
use crate::BoxError;
use crate::{model::gateway::SgGateway, model::http_route::SgHttpRoute};

use crate::service::Retrieve;

impl<F> Retrieve for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn retrieve_all_plugins(&self) -> Result<PluginInstanceMap, BoxError> {
        self.retrieve_cached(|cfg| cfg.plugins.clone()).await
    }

    async fn retrieve_plugin(&self, id: &PluginInstanceId) -> Result<Option<PluginConfig>, BoxError> {
        self.retrieve_cached(|cfg| {
            cfg.plugins.get(id).cloned().map(|spec| PluginConfig { spec, id: id.clone() })
        })
        .await
    }

    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<SgGateway>, BoxError> {
        self.retrieve_cached(|cfg| cfg.gateways.get(gateway_name).map(|item| item.gateway.clone())).await
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<SgHttpRoute>, BoxError> {
        self.retrieve_cached(|cfg| cfg.gateways.get(gateway_name).and_then(|item| item.routes.get(route_name)).cloned()).await
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, BoxError> {
        self.retrieve_cached(|cfg| cfg.gateways.get(name).map(|item| item.routes.keys().cloned().collect()).unwrap_or_default()).await
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, BoxError> {
        self.retrieve_cached(|cfg| cfg.gateways.keys().cloned().collect()).await
    }

    async fn retrieve_config(&self) -> Result<spacegate_model::Config, BoxError> {
        self.retrieve_cached(Clone::clone).await
    }
}
