use crate::service::Delete;
use crate::{service::config_format::ConfigFormat, BoxError};

use super::Fs;
impl<F> Delete for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn delete_plugin(&self, id: &spacegate_model::PluginInstanceId) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            config.plugins.remove(id);
            Ok(())
        })
        .await
    }

    async fn delete_config_item_gateway(&self, gateway_name: &str) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            config.gateways.remove(gateway_name);
            Ok(())
        })
        .await
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(gw) = config.gateways.get_mut(gateway_name) {
                gw.routes.remove(route_name);
            }
            Ok(())
        })
        .await
    }

    async fn delete_config_item(&self, gateway_name: &str) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            config.gateways.remove(gateway_name);
            Ok(())
        })
        .await
    }

    async fn delete_config_item_all_routes(&self, gateway_name: &str) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(gw) = config.gateways.get_mut(gateway_name) {
                gw.routes.clear()
            }
            Ok(())
        })
        .await
    }
}
