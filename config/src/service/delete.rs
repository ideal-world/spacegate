use std::future::Future;

use crate::{service::retrieve::Retrieve, BoxError};
mod fs;
#[cfg(feature = "k8s")]
mod k8s;
pub trait Delete: Sync + Send {
    fn delete_config_item_gateway(&self, gateway_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send;
    fn delete_config_item_all_routes(&self, gateway_name: &str) -> impl Future<Output = Result<(), BoxError>> + Send
    where
        Self: Retrieve,
    {
        async move {
            for route_name in self.retrieve_config_item_route_names(gateway_name).await? {
                self.delete_config_item_route(gateway_name, &route_name).await?;
            }
            Ok(())
        }
    }
    fn delete_config_item(&self, name: &str) -> impl Future<Output = Result<(), BoxError>> + Send
    where
        Self: Retrieve,
    {
        async move {
            self.delete_config_item_gateway(name).await?;
            self.delete_config_item_all_routes(name).await?;
            Ok(())
        }
    }
}
