use k8s_gateway_api::Gateway;
use k8s_openapi::api::core::v1::Secret;
use kube::{api::PostParams, Api};

use crate::{
    k8s_crd::{http_spaceroute, sg_filter::SgFilter},
    service::backend::k8s::K8s,
    BoxResult,
};

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

    async fn create_config_item_route(&self, _gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        let (http_spaceroute, filters) = route.to_kube_httproute_spaceroute_filters(route_name, &self.namespace);

        let http_spaceroute_api: Api<http_spaceroute::HttpSpaceroute> = self.get_namespace_api();
        http_spaceroute_api.create(&PostParams::default(), &http_spaceroute).await?;

        for filter in filters {
            let filter_api: Api<SgFilter> = self.get_namespace_api();
            filter_api.create(&PostParams::default(), &filter.into()).await?;
        }
        Ok(())
    }
}
