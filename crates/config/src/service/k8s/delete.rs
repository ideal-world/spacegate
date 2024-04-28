use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::DeleteParams, Api, ResourceExt as _};
use spacegate_model::ext::k8s::crd::{http_spaceroute::HttpSpaceroute, sg_filter::SgFilter};

use crate::{
    service::{Delete, Retrieve as _},
    BoxResult,
};

use super::{
    convert::{gateway_k8s_conv::SgGatewayConv as _, route_k8s_conv::SgHttpRouteConv as _},
    K8s,
};

impl Delete for K8s {
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        if let Some(gateway) = self.retrieve_config_item_gateway(gateway_name).await? {
            let (_, secret, delete_plugin_ids) = gateway.to_kube_gateway(&self.namespace);

            if let Some(secret) = secret {
                let secret_api: Api<Secret> = self.get_namespace_api();
                secret_api.delete(&secret.name_any(), &DeleteParams::default()).await?;
            }

            for delete_plugin_id in delete_plugin_ids {
                self.delete_plugin(&delete_plugin_id).await?;
            }

            gateway_api.delete(gateway_name, &DeleteParams::default()).await?;
        }
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<()> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        if let Some(http_route) = self.retrieve_config_item_route(gateway_name, route_name).await? {
            let (_, delete_plugin_ids) = http_route.to_kube_httproute(gateway_name, route_name, &self.namespace);
            for delete_plugin_id in delete_plugin_ids {
                self.delete_plugin(&delete_plugin_id).await?;
            }
            match http_spaceroute_api.delete(route_name, &DeleteParams::default()).await {
                Ok(_) => Ok(()),
                Err(f_e) => match httproute_api.delete(route_name, &DeleteParams::default()).await {
                    Ok(_) => Ok(()),
                    Err(s_e) => Err(format!("Failed to delete route {}: httpspaceroute: {}, httproute: {}", route_name, f_e, s_e).into()),
                },
            }
        } else {
            Ok(())
        }
    }

    async fn delete_plugin(&self, id: &spacegate_model::PluginInstanceId) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();
        match id.name.clone() {
            spacegate_model::PluginInstanceName::Anon { uid: _ } => {}
            spacegate_model::PluginInstanceName::Named { name } => {
                filter_api.delete(&name, &DeleteParams::default()).await?;
            }
            spacegate_model::PluginInstanceName::Mono => {}
        }
        Ok(())
    }
}
