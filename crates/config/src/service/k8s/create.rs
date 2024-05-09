use k8s_gateway_api::Gateway;
use k8s_openapi::api::core::v1::Secret;
use kube::{api::PostParams, Api};
use spacegate_model::{
    ext::k8s::crd::{http_spaceroute, sg_filter::SgFilter},
    BoxError, PluginInstanceId,
};

use crate::{
    service::{Create, Update},
    BoxResult,
};

use super::{
    convert::{filter_k8s_conv::PluginIdConv as _, gateway_k8s_conv::SgGatewayConv as _, route_k8s_conv::SgHttpRouteConv as _, ToTarget as _},
    K8s,
};

impl Create for K8s {
    async fn create_config_item_gateway(&self, _gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let (gateway, secret, plugin_ids) = gateway.to_kube_gateway(&self.namespace);

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        gateway_api.create(&PostParams::default(), &gateway).await?;

        if let Some(secret) = secret {
            let secret_api: Api<Secret> = self.get_namespace_api();
            secret_api.create(&PostParams::default(), &secret).await?;
        }

        for plugin_id in plugin_ids {
            plugin_id.add_filter_target(gateway.to_target_ref(), self).await?;
        }

        Ok(())
    }

    async fn create_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        let (http_spaceroute, plugin_ids) = route.to_kube_httproute(gateway_name, route_name, &self.namespace);

        let http_spaceroute_api: Api<http_spaceroute::HttpSpaceroute> = self.get_namespace_api();
        http_spaceroute_api.create(&PostParams::default(), &http_spaceroute).await?;

        for id in plugin_ids {
            id.add_filter_target(http_spaceroute.to_target_ref(), self).await?;
        }

        Ok(())
    }

    async fn create_plugin(&self, id: &PluginInstanceId, value: serde_json::Value) -> Result<(), BoxError> {
        let filter = id.to_singe_filter(value, None, &self.namespace);

        if let Some(filter) = filter {
            let filter_api: Api<SgFilter> = self.get_namespace_api();
            if filter_api.get_opt(&filter.name).await?.is_none() {
                filter_api.create(&PostParams::default(), &filter.into()).await?;
            } else {
                // do update
                self.update_plugin(id, filter.filter.config).await?;
            }
        }
        Ok(())
    }
}
