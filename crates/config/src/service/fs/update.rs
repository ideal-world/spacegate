use spacegate_model::{Config, ConfigItem, SgGateway, SgHttpRoute};

use crate::{
    service::{config_format::ConfigFormat, Update},
    BoxError,
};

use super::Fs;

impl<F> Update for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn update_plugin(&self, id: &spacegate_model::PluginInstanceId, value: serde_json::Value) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_spec) = config.plugins.get_mut(id) {
                *prev_spec = value;
                Ok(())
            } else {
                Err("plugin not exists".into())
            }
        })
        .await
    }
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                prev_item.gateway = gateway;
                Ok(())
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }
    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgHttpRoute) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                if let Some(prev_route) = prev_item.routes.get_mut(route_name) {
                    *prev_route = route;
                    Ok(())
                } else {
                    Err("route not exists".into())
                }
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }

    async fn update_config_item(&self, gateway_name: &str, item: ConfigItem) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                *prev_item = item;
                Ok(())
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }
    async fn update_config(&self, config: Config) -> Result<(), BoxError> {
        self.modify_cached(|prev_config| {
            *prev_config = config;
            Ok(())
        })
        .await
    }
}
