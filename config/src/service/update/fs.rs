use tokio::{fs::OpenOptions, io::AsyncWriteExt};

use crate::{
    service::{backend::fs::Fs, config_format::ConfigFormat},
    BoxError,
};

use super::Update;

impl<F> Update for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> Result<(), BoxError> {
        let gateway_file_path = self.gateway_path(gateway_name);
        let gateway_file = self.format.ser(&gateway)?;
        OpenOptions::new().write(true).truncate(true).create(false).open(gateway_file_path).await?.write_all(&gateway_file).await?;
        Ok(())
    }

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> Result<(), BoxError> {
        let route_file_path = self.route_path(gateway_name, route_name);
        let route_file = self.format.ser(&route)?;
        OpenOptions::new().write(true).truncate(true).create(false).open(route_file_path).await?.write_all(&route_file).await?;
        Ok(())
    }
}
