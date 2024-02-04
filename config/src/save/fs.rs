use tokio::{fs, io};

use crate::{
    backend::fs::{Fs, ROUTES_SUFFIX},
    config_format::ConfigFormat,
    model::{SgGateway, SgHttpRoute},
    Config, ConfigItem,
};

use super::Save;

impl<F> Save for Fs<F>
where
    F: ConfigFormat + Send + Sync,
    io::Error: From<F::Error>,
{
    type Error = io::Error;

    async fn save_config_item(&self, name: &str, item: &ConfigItem) -> Result<(), Self::Error> {
        let suffix = self.gateway_suffix();
        let extension = self.format.extension();
        let gateway_file_path = self.dir.join(name).with_extension(suffix);
        let gateway_file = self.format.ser::<SgGateway>(&item.gateway)?;
        fs::write(gateway_file_path, gateway_file).await?;
        let routes_dir_path = self.dir.join(name).with_extension(ROUTES_SUFFIX);
        let mut routes_dir = fs::read_dir(&routes_dir_path).await?;
        while let Some(entry) = routes_dir.next_entry().await? {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(extension) {
                fs::remove_file(path).await?;
            }
        }
        for (name, route) in &item.routes {
            let route_file_path = routes_dir_path.join(name).with_extension(extension);
            let route_file = self.format.ser::<SgHttpRoute>(route)?;
            fs::write(route_file_path, route_file).await?;
        }
        Ok(())
    }

    async fn save_config(&self, config: &Config) -> Result<Config, Self::Error> {
        for (name, item) in &config.gateways {
            self.save_config_item(name, item).await?;
        }
        Ok(config.clone())
    }
}
