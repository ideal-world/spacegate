use k8s_gateway_api::{Gateway, HttpRoute};
use kube::{api::DeleteParams, Api};

use crate::{k8s_crd::http_spaceroute::HttpSpaceroute, service::backend::k8s::K8s, BoxResult};

use super::Delete;

impl Delete for K8s {
    async fn delete_config_item_gateway(&self, gateway_name: &str) -> BoxResult<()> {
        let gateway_api: Api<Gateway> = self.get_namespace_api();

        gateway_api.delete(gateway_name, &DeleteParams::default()).await?;
        Ok(())
    }

    async fn delete_config_item_route(&self, _gateway_name: &str, route_name: &str) -> BoxResult<()> {
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let httproute_api: Api<HttpRoute> = self.get_namespace_api();

        match http_spaceroute_api.delete(route_name, &DeleteParams::default()).await {
            Ok(_) => Ok(()),
            Err(f_e) => match httproute_api.delete(route_name, &DeleteParams::default()).await {
                Ok(_) => Ok(()),
                Err(s_e) =>  Err(format!("Failed to delete route {}: httpspaceroute: {}, httproute: {}", route_name, f_e,s_e).into()),
            } ,
        }

    }
}