use std::sync::Arc;

use spacegate_model::PluginConfig;

use crate::Config;

use super::Retrieve;

/// In-memory static Config Backend
#[derive(Debug, Clone)]
pub struct Memory {
    pub config: Arc<Config>,
}

impl Memory {
    pub fn new(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}

impl Retrieve for Memory {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<spacegate_model::SgGateway>, spacegate_model::BoxError> {
        Ok(self.config.gateways.get(gateway_name).map(|item| item.gateway.clone()))
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<spacegate_model::SgHttpRoute>, spacegate_model::BoxError> {
        Ok(self.config.gateways.get(gateway_name).and_then(|item| item.routes.get(route_name).cloned()))
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, spacegate_model::BoxError> {
        Ok(self.config.gateways.get(name).map(|item| item.routes.keys().cloned().collect::<Vec<_>>()).unwrap_or_default())
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, spacegate_model::BoxError> {
        Ok(self.config.gateways.keys().cloned().collect())
    }

    async fn retrieve_all_plugins(&self) -> Result<Vec<PluginConfig>, spacegate_model::BoxError> {
        Ok(self.config.plugins.clone().into_config_vec())
    }

    async fn retrieve_plugin(&self, id: &spacegate_model::PluginInstanceId) -> Result<Option<spacegate_model::PluginConfig>, spacegate_model::BoxError> {
        Ok(self.config.plugins.get(id).map(|spec| spacegate_model::PluginConfig {
            spec: spec.clone(),
            id: id.clone(),
        }))
    }

    async fn retrieve_plugins_by_code(&self, code: &str) -> Result<Vec<PluginConfig>, spacegate_model::BoxError> {
        Ok(self
            .config
            .plugins
            .iter()
            .filter_map(|(id, spec)| {
                (id.code == code).then_some(PluginConfig {
                    spec: spec.clone(),
                    id: id.clone(),
                })
            })
            .collect())
    }
}

use super::{CreateListener, Listen};
#[derive(Debug, Clone, Default)]
struct Static;

impl Listen for Static {
    fn poll_next(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<super::ListenEvent, crate::BoxError>> {
        std::task::Poll::Pending
    }
}

impl CreateListener for Memory {
    const CONFIG_LISTENER_NAME: &'static str = "memory";

    async fn create_listener(&self) -> Result<(Config, Box<dyn super::Listen>), Box<dyn std::error::Error + Sync + Send + 'static>> {
        Ok((self.config.as_ref().clone(), Box::new(Static)))
    }
}
