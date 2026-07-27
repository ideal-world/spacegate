use crate::service::Delete;
use crate::{service::config_format::ConfigFormat, BoxError};

use super::Fs;
impl<F> Delete for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn delete_plugin(&self, id: &spacegate_model::PluginInstanceId) -> Result<(), BoxError> {
        let path = self.plugin_path(id);
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete_config_item_gateway(&self, gateway_name: &str) -> Result<(), BoxError> {
        let current_dir = self.gateway_dir().join(gateway_name);
        match tokio::fs::remove_dir_all(&current_dir).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let legacy_path = self.legacy_gateway_config_path(gateway_name);
        match tokio::fs::remove_file(legacy_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<(), BoxError> {
        let path = self.route_path(gateway_name, route_name);
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn delete_config_item(&self, gateway_name: &str) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            config.gateways.remove(gateway_name);
            Ok(())
        })
        .await
    }

    async fn delete_config_item_all_routes(&self, gateway_name: &str) -> Result<(), BoxError> {
        let routes_dir = self.routes_dir(gateway_name);
        match tokio::fs::remove_dir_all(routes_dir).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
