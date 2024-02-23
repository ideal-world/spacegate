


use crate::{
    service::backend::k8s::K8s,
    BoxResult,
};

use super::Create;

impl Create for K8s {
    async fn create_config_item_gateway(&self, _gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        gateway.to_kube_gateway(&self.namespace);
        todo!()
    }

    async fn create_config_item_route(&self, _gateway_name: &str, _route_name: &str, _route: crate::model::SgHttpRoute) -> BoxResult<()> {
        todo!()
    }
}
