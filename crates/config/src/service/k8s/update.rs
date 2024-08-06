use std::collections::HashSet;

use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{DeleteParams, PostParams},
    Api, ResourceExt,
};
use spacegate_model::{
    ext::k8s::crd::{
        http_spaceroute::HttpSpaceroute,
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
    },
    BoxError, BoxResult, PluginInstanceId,
};

use crate::service::{Retrieve as _, Update};

use super::{
    convert::{filter_k8s_conv::PluginIdConv as _, gateway_k8s_conv::SgGatewayConv as _, route_k8s_conv::SgHttpRouteConv, ToTarget},
    K8s,
};

impl Update for K8s {
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let (mut gateway, secret, update_plugin_ids) = gateway.to_kube_gateway(&self.namespace);

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        let old_gateway = self
            .retrieve_config_item_gateway(gateway_name)
            .await?
            .map(|g| g.to_kube_gateway(&self.namespace))
            .ok_or_else(|| -> BoxError { format!("[Sg.Config] gateway [{gateway_name}] not found ,update failed").into() })?;

        gateway.metadata.resource_version = gateway_api.get_metadata(gateway_name).await?.resource_version();
        gateway_api.replace(gateway_name, &PostParams::default(), &gateway).await?;

        let secret_api: Api<Secret> = self.get_namespace_api();

        if let Some(old_secret) = old_gateway.1 {
            if let Some(mut secret) = secret {
                if old_secret.name_any() == secret.name_any() {
                    secret.metadata.resource_version = old_secret.resource_version();
                    secret_api.replace(&secret.name_any(), &PostParams::default(), &secret).await?;
                } else {
                    secret_api.create(&PostParams::default(), &secret).await?;
                }
            } else {
                secret_api.delete(&old_secret.name_any(), &DeleteParams::default()).await?;
            }
        } else if let Some(secret) = secret {
            secret_api.create(&PostParams::default(), &secret).await?;
        }

        self.update_plugin_ids_changes(old_gateway.2, update_plugin_ids, gateway.to_target_ref()).await?;
        Ok(())
    }

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        let (mut http_spaceroute, update_plugin_ids) = route.to_kube_httproute(gateway_name, route_name, &self.namespace);

        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let http_route_api: Api<HttpRoute> = self.get_namespace_api();

        let old_sg_httproute = self.retrieve_config_item_route(gateway_name, route_name).await?;

        if let Some(old_route) = http_spaceroute_api.get_metadata_opt(&http_spaceroute.name_any()).await? {
            http_spaceroute.metadata.resource_version = old_route.resource_version();
            http_spaceroute_api.replace(&http_spaceroute.name_any(), &PostParams::default(), &http_spaceroute).await?;
        } else if http_route_api.get_metadata_opt(&http_spaceroute.name_any()).await?.is_some() {
            http_route_api.delete(&http_spaceroute.name_any(), &DeleteParams::default()).await?;
            http_spaceroute_api.create(&PostParams::default(), &http_spaceroute).await?;
        } else {
            return Err(format!("raw http route {route_name} not found").into());
        };

        self.update_plugin_ids_changes(
            old_sg_httproute.map(|r| r.to_kube_httproute(gateway_name, route_name, &self.namespace).1).unwrap_or_default(),
            update_plugin_ids,
            http_spaceroute.to_target_ref(),
        )
        .await?;

        Ok(())
    }

    async fn update_plugin(&self, id: &spacegate_model::PluginInstanceId, value: serde_json::Value) -> BoxResult<()> {
        let filter = id.to_singe_filter(value, None, &self.namespace);

        if let Some(filter) = filter {
            let filter_api: Api<SgFilter> = self.get_namespace_api();
            if let Some(old_filter) = filter_api.get_opt(&filter.name).await? {
                let mut update_filter: SgFilter = filter.into();
                update_filter.metadata.resource_version = old_filter.resource_version();

                filter_api.replace(&old_filter.name_any(), &PostParams::default(), &update_filter).await?;
            } else {
                return Err(format!("raw filter {id:?} not found").into());
            };
        }

        Ok(())
    }
}

impl K8s {
    pub(crate) async fn update_plugin_ids_changes(&self, old: Vec<PluginInstanceId>, update: Vec<PluginInstanceId>, target: K8sSgFilterSpecTargetRef) -> BoxResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().collect();
        let update_set: HashSet<_> = update.into_iter().collect();

        let add_vec: Vec<_> = update_set.difference(&old_set).collect();
        for add_id in add_vec {
            add_id.add_filter_target(target.clone(), self).await?;
        }
        let delete_vec: Vec<_> = old_set.difference(&update_set).collect();
        for delete_id in delete_vec {
            delete_id.remove_filter_target(target.clone(), self).await?;
        }

        Ok(())
    }
}
