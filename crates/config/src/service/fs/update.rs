use spacegate_model::{Config, ConfigItem, PluginConfig, SgGateway, SgRoute};

use crate::{
    service::{config_format::ConfigFormat, encode_stored_plugin_config, Update},
    BoxError,
};

use super::Fs;

impl<F> Update for Fs<F>
where
    F: ConfigFormat + Send + Sync,
{
    async fn update_plugin(&self, config: PluginConfig) -> Result<(), BoxError> {
        // 仅更新单个插件 JSON，避免 modify_cached 清空整棵配置树（Docker 共享挂载会 EBUSY/EROFS）
        tokio::fs::create_dir_all(self.plugin_dir()).await?;
        let path = self.plugin_path(&config.id);
        let b_spec = self.format.ser(&encode_stored_plugin_config(config)?)?;
        tokio::fs::write(&path, &b_spec).await?;
        Ok(())
    }
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                prev_item.gateway = gateway;
                Ok(())
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }
    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgRoute) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                if let Some(prev_route) = prev_item.routes.get_mut(route_name) {
                    *prev_route = route;
                    Ok(())
                } else {
                    Err("route not exists".into())
                }
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }

    async fn update_config_item(&self, gateway_name: &str, item: ConfigItem) -> Result<(), BoxError> {
        self.modify_cached(|config| {
            if let Some(prev_item) = config.gateways.get_mut(gateway_name) {
                *prev_item = item;
                Ok(())
            } else {
                Err("item not exists".into())
            }
        })
        .await
    }
    async fn update_config(&self, config: Config) -> Result<(), BoxError> {
        self.modify_cached(|prev_config| {
            *prev_config = config;
            Ok(())
        })
        .await
    }
}
