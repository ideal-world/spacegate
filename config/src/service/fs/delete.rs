use crate::service::Delete;
use crate::{service::config_format::ConfigFormat, BoxError};

use super::Fs;
impl<F> Delete for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> Result<(), BoxError> {
        let gateway_file_path = self.gateway_path(gateway_name);
        tokio::fs::remove_file(gateway_file_path).await?;
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<(), BoxError> {
        let route_file_path = self.route_path(gateway_name, route_name);
        tokio::fs::remove_file(route_file_path).await?;
        Ok(())
    }
}
