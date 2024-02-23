use crate::service::backend::memory::Memory;

use super::Retrieve;

impl Retrieve for Memory {
    async fn retrieve_config_item_gateway(&self, gateway_name: &str) -> Result<Option<crate::model::SgGateway>, crate::BoxError> {
        Ok(self.config.read().await.gateways.get(gateway_name).map(|x| x.gateway.clone()))
    }

    async fn retrieve_config_item_route(&self, gateway_name: &str, route_name: &str) -> Result<Option<crate::model::SgHttpRoute>, crate::BoxError> {
        Ok(self.config.read().await.gateways.get(gateway_name).and_then(|x| x.routes.get(route_name).cloned()))
    }

    async fn retrieve_config_item_route_names(&self, name: &str) -> Result<Vec<String>, crate::BoxError> {
        Ok(self.config.read().await.gateways.get(name).map(|x| x.routes.keys().cloned().collect::<Vec<_>>()).unwrap_or_default())
    }

    async fn retrieve_config_names(&self) -> Result<Vec<String>, crate::BoxError> {
        Ok(self.config.read().await.gateways.keys().cloned().collect())
    }
}
