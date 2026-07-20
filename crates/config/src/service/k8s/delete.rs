use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::DeleteParams, Api, ResourceExt as _};
use spacegate_model::ext::k8s::crd::{http_spaceroute::HttpSpaceroute, mcp_route::McpRoute, sg_filter::SgFilter};

use crate::{
    service::{Delete, Retrieve as _},
    BoxResult,
};

use super::{
    convert::{filter_k8s_conv::PluginIdConv, gateway_k8s_conv::SgGatewayConv as _, route_k8s_conv::KubeRoute, route_k8s_conv::SgRouteK8sConv as _, ToTarget as _},
    K8s,
};

impl Delete for K8s {
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        if let Some(sg_gateway) = self.retrieve_config_item_gateway(gateway_name).await? {
            let (gateway, secret, delete_plugin_ids) = sg_gateway.to_kube_gateway(&self.namespace, &self.gateway_class_name);

            if let Some(secret) = secret {
                let secret_api: Api<Secret> = self.get_namespace_api();
                secret_api.delete(&secret.name_any(), &DeleteParams::default()).await?;
            }

            for delete_plugin_id in delete_plugin_ids {
                delete_plugin_id.remove_filter_target(gateway.to_target_ref(), self).await?;
            }

            gateway_api.delete(gateway_name, &DeleteParams::default()).await?;
        }
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<()> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();
        let mcp_route_api: Api<McpRoute> = self.get_namespace_api();

        if let Some(sg_http_route) = self.retrieve_config_item_route(gateway_name, route_name).await? {
            let route = sg_http_route.to_kube_route(gateway_name, route_name, &self.namespace);
            let target_ref = route.to_target_ref();
            for delete_plugin_id in route.plugin_ids() {
                delete_plugin_id.remove_filter_target(target_ref.clone(), self).await?;
            }
            match route {
                KubeRoute::Http(_, _) => match http_spaceroute_api.delete(route_name, &DeleteParams::default()).await {
                    Ok(_) => Ok(()),
                    Err(f_e) => match httproute_api.delete(route_name, &DeleteParams::default()).await {
                        Ok(_) => Ok(()),
                        Err(s_e) => Err(format!("Failed to delete route {}: httpspaceroute: {}, httproute: {}", route_name, f_e, s_e).into()),
                    },
                },
                KubeRoute::Mcp(_, _) => mcp_route_api.delete(route_name, &DeleteParams::default()).await.map(|_| ()).map_err(Into::into),
            }
        } else {
            Ok(())
        }
    }

    async fn delete_plugin(&self, id: &spacegate_model::PluginInstanceId) -> BoxResult<()> {
        let filter_api: Api<SgFilter> = self.get_namespace_api();
        if let Ok(filter) = filter_api.get(&id.name.to_raw_str()).await {
            if filter.spec.target_refs.is_empty() {
                filter_api.delete(&filter.name_any(), &DeleteParams::default()).await?;
            } else {
                return Err(format!(
                    "The filter have references [{}]",
                    filter.spec.target_refs.iter().map(|t| format!("{}", t)).collect::<Vec<String>>().join(", ")
                )
                .into());
            }
        }
        Ok(())
    }
}
