use std::{collections::BTreeMap, ffi::OsString, future::Future};

use tokio::{fs, io};

use crate::backend::fs::{Fs, GATEWAY_SUFFIX, ROUTES_SUFFIX};
use crate::config_format::ConfigFormat;
use crate::{model::gateway::SgGateway, model::http_route::SgHttpRoute, Config, ConfigItem};

use super::Retrieve;

impl<F> Retrieve for Fs<F>
where
    F: ConfigFormat + Send + Sync,
    io::Error: From<F::Error>,
{
    type Error = io::Error;

    async fn retrieve_config_item(&self, name: &str) -> Result<Option<ConfigItem>, Self::Error> {
        let dir = self.dir.as_ref();
        let mut gateway_path = dir.join(name);
        let extension = self.format.extension();
        gateway_path.set_extension(self.gateway_suffix());
        let gateway_file = match fs::read(&gateway_path).await {
            Ok(f) => f,
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    return Ok(None);
                }
                return Err(e);
            }
        };
        let gateway = self.format.de::<SgGateway>(&gateway_file)?;
        let mut routes_dir = fs::read_dir(dir.join(name).with_extension(ROUTES_SUFFIX)).await?;
        let mut routes = BTreeMap::new();
        while let Ok(Some(entry)) = routes_dir.next_entry().await {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(extension) {
                let Some(route_name) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
                    continue;
                };
                let route_file = fs::read(path).await?;
                let route = self.format.de::<SgHttpRoute>(&route_file)?;
                routes.insert(route_name, route);
            }
        }

        Ok(Some(ConfigItem { gateway, routes }))
    }

    async fn retrieve_config(&self) -> Result<Config, Self::Error> {
        let mut gateway_dir = fs::read_dir(self.dir.as_ref()).await?;
        let mut gateways = BTreeMap::new();
        let extension = self.format.extension();
        while let Ok(Some(entry)) = gateway_dir.next_entry().await {
            let path = entry.path();
            if path.is_file() && path.extension() == Some(extension) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.strip_suffix(GATEWAY_SUFFIX)).and_then(|s| s.strip_suffix('.')) {
                    let item = self.retrieve_config_item(name).await?;
                    if let Some(item) = item {
                        gateways.insert(name.to_string(), item);
                    }
                }
            }
        }

        Ok(Config { gateways })
    }
}
