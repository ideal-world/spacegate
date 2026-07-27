use spacegate_model::{ConfigItem, PluginConfig, SgRoute};

use crate::{model::SgGateway, service::config_format::ConfigFormat, BoxError};

use crate::service::{encode_stored_plugin_config, Create};

use super::Fs;

impl<F> Create for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn create_plugin(&self, config: PluginConfig) -> Result<(), BoxError> {
        let path = self.plugin_path(&config.id);
        if path.exists() {
            return Err("plugin existed".into());
        }
        // 仅写入新插件文件，避免 rewrite 整个 /etc/spacegate
        tokio::fs::create_dir_all(self.plugin_dir()).await?;
        let b_spec = self.format.ser(&encode_stored_plugin_config(config)?)?;
        tokio::fs::write(&path, &b_spec).await?;
        Ok(())
    }
    async fn create_config_item(&self, gateway_name: &str, item: ConfigItem) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if config.gateways.contains_key(gateway_name) {
                return Err("item existed".into());
            }
            config.gateways.insert(gateway_name.into(), item);
            Ok(())
        })
        .await
    }
    async fn create_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> Result<(), BoxError> {
        if self.existing_gateway_config_path(gateway_name).is_some() {
            return Err("item existed".into());
        }
        let path = self.gateway_main_config_path(gateway_name);
        let config = ConfigItem {
            gateway,
            routes: Default::default(),
        };
        tokio::fs::create_dir_all(path.parent().expect("gateway config path has a parent")).await?;
        tokio::fs::write(path, self.format.ser(&config)?).await?;
        Ok(())
    }
    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgRoute) -> Result<(), BoxError> {
        if self.existing_gateway_config_path(gateway_name).is_none() {
            return Err("gateway not exists".into());
        }
        let path = self.route_path(gateway_name, route_name);
        if path.exists() {
            return Err("route existed".into());
        }
        tokio::fs::create_dir_all(self.routes_dir(gateway_name)).await?;
        tokio::fs::write(path, self.format.ser(&route)?).await?;
        Ok(())
    }
}
