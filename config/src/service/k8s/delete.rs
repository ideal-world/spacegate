use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{api::DeleteParams, Api, ResourceExt as _};

use crate::{
    k8s_crd::http_spaceroute::HttpSpaceroute,
    service::{backend::k8s::K8s, Retrieve as _},
    BoxResult,
};

use super::Delete;

impl Delete for K8s {
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        if let Some(gateway) = self.retrieve_config_item_gateway(gateway_name).await? {
            let (_, secret, delete_filter) = gateway.to_kube_gateway(&self.namespace);

            if let Some(secret) = secret {
                let secret_api: Api<Secret> = self.get_namespace_api();
                secret_api.delete(&secret.name_any(), &DeleteParams::default()).await?;
            }

            self.delete_sgfilter_vec(delete_filter.iter().collect()).await?;
            gateway_api.delete(gateway_name, &DeleteParams::default()).await?;
        }
        Ok(())
    }

    async fn delete_config_item_route(&self, gateway_name: &str, route_name: &str) -> BoxResult<()> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        if let Some(http_route) = self.retrieve_config_item_route(gateway_name, route_name).await? {
            let (_, delete_filter) = http_route.to_kube_httproute_spaceroute_filters(route_name, &self.namespace);
            self.delete_sgfilter_vec(delete_filter.iter().collect()).await?;

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
}
