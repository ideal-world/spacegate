use tokio::{
    fs::{self, OpenOptions},
    io::{self, AsyncWriteExt},
};

use crate::{
    model::{SgGateway, SgHttpRoute},
    service::{backend::fs::Fs, config_format::ConfigFormat},
    BoxError,
};

use super::Create;

impl<F> Create for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: &SgGateway) -> Result<(), BoxError> {
        let bin = self.format.ser::<SgGateway>(gateway)?;
        OpenOptions::new().truncate(false).create_new(true).write(true).open(self.gateway_path(gateway_name)).await?.write_all(&bin).await?;
        let routes_dir_path = self.routes_dir(gateway_name);
        if let Err(e) = fs::create_dir(&routes_dir_path).await {
            if e.kind() != io::ErrorKind::AlreadyExists {
                return Err(Box::new(e));
            }
        }
        Ok(())
    }
    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: &SgHttpRoute) -> Result<(), BoxError> {
        let bin = self.format.ser::<SgHttpRoute>(route)?;
        OpenOptions::new().truncate(false).create_new(true).write(true).open(self.route_path(gateway_name, route_name)).await?.write_all(&bin).await?;
        Ok(())
    }
}
