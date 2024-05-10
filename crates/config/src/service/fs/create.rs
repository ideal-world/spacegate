use spacegate_model::ConfigItem;

use crate::{
    model::{SgGateway, SgHttpRoute},
    service::config_format::ConfigFormat,
    BoxError,
};

use crate::service::Create;

use super::Fs;

impl<F> Create for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn create_plugin(&self, id: &spacegate_model::PluginInstanceId, value: serde_json::Value) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if config.plugins.get(id).is_some() {
                return Err("plugin existed".into());
            }
            config.plugins.insert(id.clone(), value);
            Ok(())
        })
        .await
    }
    async fn create_config_item(&self, gateway_name: &str, item: ConfigItem) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if config.gateways.contains_key(gateway_name) {
                return Err("item existed".into());
            }
            config.gateways.insert(gateway_name.into(), item);
            Ok(())
        })
        .await
    }
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> Result<(), BoxError> {
        self.create_config_item(
            gateway_name,
            ConfigItem {
                gateway,
                routes: Default::default(),
            },
        )
        .await
    }
    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgHttpRoute) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(item) = config.gateways.get_mut(gateway_name) {
                if item.routes.contains_key(gateway_name) {
                    Err("route existed".into())
                } else {
                    item.routes.insert(route_name.to_string(), route);
                    Ok(())
                }
            } else {
                Err("gateway not exists".into())
            }
        })
        .await
    }
}
