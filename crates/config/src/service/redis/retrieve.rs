use std::collections::HashMap;

use redis::AsyncCommands as _;

use crate::{
    model::{SgGateway, SgHttpRoute},
    service::{
        backend::redis::{Redis, CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY},
        config_format::ConfigFormat,
    },
    BoxResult,
};

use super::Retrieve;

impl<F> Retrieve for Redis<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> BoxResult<Option<SgGateway>> {
        let gateway_config: Option<String> = self.get_con().await?.hget(CONF_GATEWAY_KEY, gateway_name).await?;
        gateway_config.map(|config| self.format.de::<SgGateway>(config.as_bytes()).map_err(|e| format!("[SG.Config] Gateway Config parse error {}", e).into())).transpose()
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<Option<crate::model::SgHttpRoute>> {
        let http_route_config: Option<String> = self.get_con().await?.hget(&format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_name), route_name).await?;
        http_route_config.map(|config| self.format.de::<SgHttpRoute>(config.as_bytes()).map_err(|e| format!("[SG.Config] Route Config parse error {}", e).into())).transpose()
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> BoxResult<Vec<String>> {
        let http_route_configs: HashMap<String, String> = self.get_con().await?.hgetall(&format!("{CONF_HTTP_ROUTE_KEY}{}", name)).await?;

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
}
