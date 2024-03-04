use std::future::Future;

use crate::{
    model::{SgGateway, SgHttpRoute},
    BoxError, Config, ConfigItem,
};

pub mod fs;
#[cfg(feature = "k8s")]
pub mod k8s;

pub trait Update: Sync + Send {
    fn update_config_item_gateway(&self, gateway_name: &str, gateway: SgGateway) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: SgHttpRoute) -> impl Future<Output = Result<(), BoxError>> + Send;

    fn update_config_item(&self, name: &str, item: ConfigItem) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            self.update_config_item_gateway(name, item.gateway).await?;
            for (route_name, route) in item.routes {
                self.update_config_item_route(name, &route_name, route).await?;
            }
            Ok(())
        }
    }
    fn update_config(&self, config: Config) -> impl Future<Output = Result<(), BoxError>> + Send {
        async move {
            for (name, item) in config.gateways {
                self.update_config_item(&name, item).await?;
            }
            Ok(())
        }
    }
}
