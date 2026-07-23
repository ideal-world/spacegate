use std::collections::{HashMap, HashSet};

use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{DeleteParams, PostParams},
    Api, ResourceExt,
};
use spacegate_model::{
    ext::k8s::crd::{
        http_spaceroute::HttpSpaceroute,
        mcp_route::McpRoute,
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
    },
    BoxError, BoxResult, PluginConfig,
};

use crate::service::{Retrieve as _, Update};

use super::{
    convert::{filter_k8s_conv::PluginIdConv as _, gateway_k8s_conv::SgGatewayConv as _, route_k8s_conv::KubeRoute, route_k8s_conv::SgRouteK8sConv, ToTarget},
    K8s,
};

impl Update for K8s {
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let (mut gateway, secret, update_plugin_ids) = gateway.to_kube_gateway(&self.namespace, &self.gateway_class_name);

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        let old_gateway = self
            .retrieve_config_item_gateway(gateway_name)
            .await?
            .map(|g| g.to_kube_gateway(&self.namespace, &self.gateway_class_name))
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

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgRoute) -> BoxResult<()> {
        let mut kube_route = route.to_kube_route(gateway_name, route_name, &self.namespace);

        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let http_route_api: Api<HttpRoute> = self.get_namespace_api();
        let mcp_route_api: Api<McpRoute> = self.get_namespace_api();

        let old_sg_httproute = self.retrieve_config_item_route(gateway_name, route_name).await?;

        match &mut kube_route {
            KubeRoute::Http(http_spaceroute, _) => {
                if let Some(old_route) = http_spaceroute_api.get_metadata_opt(&http_spaceroute.name_any()).await? {
                    http_spaceroute.metadata.resource_version = old_route.resource_version();
                    http_spaceroute_api.replace(&http_spaceroute.name_any(), &PostParams::default(), http_spaceroute).await?;
                } else if http_route_api.get_metadata_opt(&http_spaceroute.name_any()).await?.is_some() {
                    http_route_api.delete(&http_spaceroute.name_any(), &DeleteParams::default()).await?;
                    http_spaceroute_api.create(&PostParams::default(), http_spaceroute).await?;
                } else if mcp_route_api.get_metadata_opt(&http_spaceroute.name_any()).await?.is_some() {
                    mcp_route_api.delete(&http_spaceroute.name_any(), &DeleteParams::default()).await?;
                    http_spaceroute_api.create(&PostParams::default(), http_spaceroute).await?;
                } else {
                    return Err(format!("raw route {route_name} not found").into());
                };
            }
            KubeRoute::Mcp(mcp_route, _) => {
                if let Some(old_route) = mcp_route_api.get_metadata_opt(&mcp_route.name_any()).await? {
                    mcp_route.metadata.resource_version = old_route.resource_version();
                    mcp_route_api.replace(&mcp_route.name_any(), &PostParams::default(), mcp_route).await?;
                } else if http_spaceroute_api.get_metadata_opt(&mcp_route.name_any()).await?.is_some() {
                    http_spaceroute_api.delete(&mcp_route.name_any(), &DeleteParams::default()).await?;
                    mcp_route_api.create(&PostParams::default(), mcp_route).await?;
                } else if http_route_api.get_metadata_opt(&mcp_route.name_any()).await?.is_some() {
                    http_route_api.delete(&mcp_route.name_any(), &DeleteParams::default()).await?;
                    mcp_route_api.create(&PostParams::default(), mcp_route).await?;
                } else {
                    return Err(format!("raw route {route_name} not found").into());
                };
            }
        };

        self.update_plugin_ids_changes(
            old_sg_httproute.map(|r| r.to_kube_route(gateway_name, route_name, &self.namespace).plugin_bindings().to_vec()).unwrap_or_default(),
            kube_route.plugin_bindings().to_vec(),
            kube_route.to_target_ref(),
        )
        .await?;

        Ok(())
    }

    async fn update_plugin(&self, config: PluginConfig) -> BoxResult<()> {
        let id = config.id.clone();
        let filter = config.id.to_singe_filter(config.spec, config.display_name, None, &self.namespace);

        if let Some(filter) = filter {
            let filter_api: Api<SgFilter> = self.get_namespace_api();
            if let Some(old_filter) = filter_api.get_opt(&filter.name).await? {
                let name = &old_filter.name_any();
                let mut update_filter: SgFilter = filter.into();
                update_filter.metadata.resource_version = old_filter.resource_version();
                update_filter.spec.target_refs = old_filter.spec.target_refs;
                filter_api.replace(name, &PostParams::default(), &update_filter).await?;
            } else {
                return Err(format!("raw filter {id:?} not found").into());
            };
        }

        Ok(())
    }
}

impl K8s {
    pub(crate) async fn update_plugin_ids_changes(
        &self,
        old: Vec<spacegate_model::PluginBinding>,
        update: Vec<spacegate_model::PluginBinding>,
        target: K8sSgFilterSpecTargetRef,
    ) -> BoxResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().map(|binding| binding.id).collect();
        let update_set: HashMap<_, _> = update.into_iter().map(|binding| (binding.id, binding.priority)).collect();

        for (id, priority) in &update_set {
            id.add_filter_target(target.clone(), *priority, self).await?;
        }
        let update_ids: HashSet<_> = update_set.into_keys().collect();
        let delete_vec: Vec<_> = old_set.difference(&update_ids).collect();
        for delete_id in delete_vec {
            delete_id.remove_filter_target(target.clone(), self).await?;
        }

        Ok(())
    }
}
