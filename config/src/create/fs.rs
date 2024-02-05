use tokio::{
    fs::{self, OpenOptions},
    io::{self, AsyncWriteExt},
};

use crate::{
    backend::fs::{Fs, ROUTES_SUFFIX},
    config_format::ConfigFormat,
    model::{SgGateway, SgHttpRoute},
    Config, ConfigItem,
};

use super::Create;

impl<F> Create for Fs<F>
where
    F: ConfigFormat + Send + Sync,
    io::Error: From<F::Error>,
{
    type Error = io::Error;
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: &SgGateway) -> Result<(), Self::Error> {
        let bin = self.format.ser::<SgGateway>(gateway)?;
        OpenOptions::new().truncate(false).create(true).write(true).open(self.gateway_path(gateway_name)).await?.write_all(&bin).await?;
        let routes_dir_path = self.routes_dir(gateway_name);
        fs::create_dir(&routes_dir_path).await?;
        Ok(())
    }
    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: &SgHttpRoute) -> Result<(), Self::Error> {
        let bin = self.format.ser::<SgHttpRoute>(route)?;
        OpenOptions::new().truncate(false).create(true).write(true).open(self.route_path(gateway_name, route_name)).await?.write_all(&bin).await?;
        Ok(())
    }
}
