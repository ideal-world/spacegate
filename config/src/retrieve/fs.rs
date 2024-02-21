use std::ffi::OsStr;
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

    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<SgGateway>, Self::Error> {
        let gateway_file_path = self.gateway_path(gateway_name);
        if !gateway_file_path.exists() {
            return Ok(None);
        }
        let gateway_file = fs::read(gateway_file_path).await?;
        Ok(Some(self.format.de(&gateway_file)?))
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<SgHttpRoute>, Self::Error> {
        let route_file_path = self.route_path(gateway_name, route_name);
        if !route_file_path.exists() {
            return Ok(None);
        }
        let route_file = fs::read(route_file_path).await?;
        Ok(Some(self.format.de(&route_file)?))
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, Self::Error> {
        let mut route_names = Vec::new();
        let mut entries = fs::read_dir(self.routes_dir(name)).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.path();
            if file_name.is_file() && file_name.extension() == Some(self.format.extension()) {
                if let Some(file_name) = file_name.file_stem().and_then(OsStr::to_str) {
                    route_names.push(file_name.to_string());
                }
            }
        }
        Ok(route_names)
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, Self::Error> {
        let mut gateway_names = Vec::new();
        let mut entries = fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.path().is_file() {
                continue;
            }
            if let Some(file_name) = entry.path().file_stem().and_then(OsStr::to_str) {
                if let Some(file_name) = file_name.strip_suffix(GATEWAY_SUFFIX).and_then(|f| f.strip_suffix('.')) {
                    gateway_names.push(file_name.to_owned());
                }
            }
        }
        Ok(gateway_names)
    }
}
