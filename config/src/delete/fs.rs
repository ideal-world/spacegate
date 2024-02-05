use tokio::io;

use crate::{backend::fs::Fs, config_format::ConfigFormat};

impl<F> super::Delete for Fs<F>
where
    F: ConfigFormat + Send + Sync,
    io::Error: From<F::Error>,
{
    type Error = std::io::Error;

    async fn delete_config_item_gateway(&self, gateway_name: &str) -> Result<(), Self::Error> {
        let gateway_file_path = self.gateway_path(gateway_name);
        tokio::fs::remove_file(gateway_file_path).await?;
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<(), Self::Error> {
        let route_file_path = self.route_path(gateway_name, route_name);
        tokio::fs::remove_file(route_file_path).await?;
        Ok(())
    }
}
