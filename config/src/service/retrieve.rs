use std::{collections::BTreeMap, future::Future};

use crate::{
    model::{SgGateway, SgHttpRoute},
    BoxError, Config, ConfigItem,
};

mod fs;
#[cfg(feature = "k8s")]
mod k8s;
mod memory;
#[cfg(feature = "redis")]
mod redis;

pub trait Retrieve: Sync + Send {
    fn retrieve_config_item_gateway(&self, gateway_name: &str) -> impl Future<Output = Result<Option<SgGateway>, BoxError>> + Send;
    fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> impl Future<Output = Result<Option<SgHttpRoute>, BoxError>> + Send;
    fn retrieve_config_item_route_names(&self, name: &str) -> impl Future<Output = Result<Vec<String>, BoxError>> + Send;
    fn retrieve_config_item_all_routes(&self, name: &str) -> impl Future<Output = Result<BTreeMap<String, SgHttpRoute>, BoxError>> + Send {
        async move {
            let mut routes = BTreeMap::new();
            for route_name in self.retrieve_config_item_route_names(name).await? {
                if let Ok(Some(route)) = self.retrieve_config_item_route(name, &route_name).await {
                    routes.insert(route_name, route);
                }
            }
            Ok(routes)
        }
    }
    fn retrieve_config_item(&self, name: &str) -> impl Future<Output = Result<Option<ConfigItem>, BoxError>> + Send {
        async move {
            let Some(gateway) = self.retrieve_config_item_gateway(name).await? else {
                return Ok(None);
            };
            let routes = self.retrieve_config_item_all_routes(name).await?;
            Ok(Some(ConfigItem { gateway, routes }))
        }
    }
    fn retrieve_config_names(&self) -> impl Future<Output = Result<Vec<String>, BoxError>> + Send;
    fn retrieve_config(&self) -> impl Future<Output = Result<Config, BoxError>> + Send
    where
        Self: Sync,
        BoxError: Send,
    {
        async move {
            let mut gateways = BTreeMap::new();
            for name in self.retrieve_config_names().await? {
                if let Some(item) = self.retrieve_config_item(&name).await? {
                    gateways.insert(name, item);
                }
            }
            Ok(Config { gateways })
        }
    }
}
