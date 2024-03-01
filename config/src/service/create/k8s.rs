use k8s_gateway_api::Gateway;
use k8s_openapi::api::core::v1::Secret;
use kube::{api::PostParams, Api};

use crate::{k8s_crd::sg_filter::SgFilter, service::backend::k8s::K8s, BoxResult};

use super::Create;

impl Create for K8s {
    async fn create_config_item_gateway(&self, _gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let (gateway, secret, filters) = gateway.to_kube_gateway(&self.namespace);

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        gateway_api.create(&PostParams::default(), &gateway).await?;

        if let Some(secret) = secret {
            let secret_api: Api<Secret> = self.get_namespace_api();
            secret_api.create(&PostParams::default(), &secret).await?;
        }

        for filter in filters {
            let filter_api: Api<SgFilter> = self.get_namespace_api();
            filter_api.create(&PostParams::default(), &filter.into()).await?;
        }
        Ok(())
    }

    async fn create_config_item_route(&self, _gateway_name: &str, _route_name: &str, _route: crate::model::SgHttpRoute) -> BoxResult<()> {
        todo!()
    }
}
