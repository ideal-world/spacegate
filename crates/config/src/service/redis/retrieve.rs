use std::collections::HashMap;

use super::{decode_plugin_config_entry, decode_plugin_config_value, Redis, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY, CONF_PLUGIN_KEY};
use crate::{
    model::{SgGateway, SgRoute},
    service::config_format::ConfigFormat,
    BoxResult,
};
use redis::AsyncCommands as _;
use spacegate_model::{PluginConfig, PluginInstanceId};

use crate::service::Retrieve;

impl<F> Retrieve for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> BoxResult<Option<SgGateway>> {
        let gateway_config: Option<String> = self.get_con().await?.hget(CONF_GATEWAY_KEY, gateway_name).await?;
        gateway_config.map(|config| self.format.de::<SgGateway>(config.as_bytes()).map_err(|e| format!("[SG.Config] Gateway Config parse error {}", e).into())).transpose()
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<Option<SgRoute>> {
        let http_route_config: Option<String> = self.get_con().await?.hget(format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_name), route_name).await?;
        http_route_config.map(|config| self.format.de::<SgRoute>(config.as_bytes()).map_err(|e| format!("[SG.Config] Route Config parse error {}", e).into())).transpose()
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> BoxResult<Vec<String>> {
        let http_route_configs: HashMap<String, String> = self.get_con().await?.hgetall(format!("{CONF_HTTP_ROUTE_KEY}{}", name)).await?;

        Ok(http_route_configs.into_keys().collect())
    }

    async fn retrieve_config_names(&self) -> BoxResult<Vec<String>> {
        let gateway_configs: HashMap<String, String> = self.get_con().await?.hgetall(CONF_GATEWAY_KEY).await?;

        let gateway_configs = gateway_configs
            .into_values()
            .map(|v| self.format.de(v.as_bytes()).map_err(|e| format!("[SG.Config] Gateway Config parse error {}", e).into()))
            .collect::<BoxResult<Vec<SgGateway>>>()?;

        let gateway_names = gateway_configs.into_iter().map(|g| g.name).collect();
        Ok(gateway_names)
    }

    async fn retrieve_all_plugins(&self) -> BoxResult<Vec<PluginConfig>> {
        let plugin_configs: HashMap<String, String> = self.get_con().await?.hgetall(CONF_PLUGIN_KEY).await?;

        let plugin_configs = plugin_configs
            .into_iter()
            .map(|(key, value)| {
                let value = self.format.de(value.as_bytes()).map_err(|error| -> spacegate_model::BoxError { format!("[SG.Config] Plugin Config parse error {error}").into() })?;
                decode_plugin_config_entry(&key, value)
            })
            .collect::<BoxResult<Vec<PluginConfig>>>()?;
        Ok(plugin_configs)
    }

    async fn retrieve_plugin(&self, id: &PluginInstanceId) -> BoxResult<Option<PluginConfig>> {
        let plugin_config: Option<String> = self.get_con().await?.hget(CONF_PLUGIN_KEY, id.to_string()).await?;
        plugin_config
            .map(|config| {
                let value = self.format.de(config.as_bytes()).map_err(|error| -> spacegate_model::BoxError { format!("[SG.Config] Plugin Config parse error {error}").into() })?;
                decode_plugin_config_value(id, value)
            })
            .transpose()
    }

    async fn retrieve_plugins_by_code(&self, code: &str) -> Result<Vec<PluginConfig>, spacegate_model::BoxError> {
        let plugin_configs: HashMap<String, String> = self.get_con().await?.hgetall(CONF_PLUGIN_KEY).await?;

        let plugin_configs = plugin_configs
            .into_iter()
            .filter_map(|(key, value)| {
                let value = self.format.de(value.as_bytes()).ok()?;
                decode_plugin_config_entry(&key, value).ok()
            })
            .filter(|config| config.id.code == code)
            .collect::<Vec<_>>();
        Ok(plugin_configs)
    }
}
