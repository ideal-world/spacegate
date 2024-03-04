use std::{
    collections::{HashMap, HashSet},
    mem,
};

use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{DeleteParams, ListParams, PostParams},
    Api, ResourceExt,
};

use crate::{
    k8s_crd::{
        http_spaceroute,
        sg_filter::{K8sSgFilterSpecFilter, SgFilter},
    },
    model::helper_filter::SgSingeFilter,
    service::{backend::k8s::K8s, Retrieve as _},
    BoxResult,
};

use super::Update;

impl Update for K8s {
    async fn update_config_item_gateway(&self, gateway_name: &str, gateway: crate::model::SgGateway) -> BoxResult<()> {
        let (mut gateway, secret, update_filter) = gateway.to_kube_gateway(&self.namespace);

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        let old_gateway = self.retrieve_config_item_gateway(gateway_name).await?;

        gateway.metadata.resource_version = gateway_api.get_metadata(gateway_name).await?.resource_version();
        gateway_api.replace(gateway_name, &PostParams::default(), &gateway).await?;

        if let Some(mut secret) = secret {
            let secret_api: Api<Secret> = self.get_namespace_api();
            secret.metadata.resource_version = secret_api.get_metadata(&secret.name_any()).await?.resource_version();
            secret_api.replace(&secret.name_any(), &PostParams::default(), &secret).await?;
        }

        self.update_filter_changes(old_gateway.map(|r| r.to_kube_gateway(&self.namespace).2).unwrap_or_default(), update_filter).await?;

        Ok(())
    }

    async fn update_config_item_route(&self, gateway_name: &str, route_name: &str, route: crate::model::SgHttpRoute) -> BoxResult<()> {
        let (mut http_spaceroute, update_filter) = route.to_kube_httproute_spaceroute_filters(route_name, &self.namespace);

        let http_spaceroute_api: Api<http_spaceroute::HttpSpaceroute> = self.get_namespace_api();
        let http_route_api: Api<HttpRoute> = self.get_namespace_api();

        let old_sg_httproute = self.retrieve_config_item_route(gateway_name, route_name).await?;

        http_spaceroute.metadata.resource_version = if let Some(old_route) = http_spaceroute_api.get_metadata_opt(&http_spaceroute.name_any()).await? {
            old_route.resource_version()
        } else {
            if let Some(old_route) = http_route_api.get_metadata_opt(&http_spaceroute.name_any()).await? {
                old_route.resource_version()
            } else {
                return Err(format!("raw http route {route_name} not found").into());
            }
        };

        http_spaceroute_api.replace(&http_spaceroute.name_any(), &PostParams::default(), &http_spaceroute).await?;

        self.update_filter_changes(
            old_sg_httproute.map(|r| r.to_kube_httproute_spaceroute_filters(route_name, &self.namespace).1).unwrap_or_default(),
            update_filter,
        )
        .await?;

        Ok(())
    }
}

impl K8s {
    pub(crate) async fn update_filter_changes(&self, old: Vec<SgSingeFilter>, update: Vec<SgSingeFilter>) -> BoxResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().collect();
        let update_set: HashSet<_> = update.into_iter().collect();

        let update_vec: Vec<_> = old_set.intersection(&update_set).collect();
        self.update_sgfilter_vec(update_vec).await?;
        let add_vec: Vec<_> = update_set.difference(&old_set).collect();
        self.add_sgfilter_vec(add_vec).await?;
        let delete_vec: Vec<_> = old_set.difference(&update_set).collect();
        self.delete_sgfilter_vec(delete_vec).await?;

        Ok(())
    }

    pub async fn add_sgfilter_vec(&self, sgfilters: Vec<&SgSingeFilter>) -> BoxResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = self.get_namespace_api();

            let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                filter_list
            } else {
                let filter_list = filter_api.list(&ListParams::default()).await?;
                filter_map.insert(sf.namespace.clone(), filter_list);
                filter_map.get(&sf.namespace).expect("")
            };

            if let Some(mut query_sf) = namespace_filter.items.clone().into_iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)) {
                if !query_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                    query_sf.spec.target_refs.push(sf.target_ref.clone());
                    Self::replace_filter(query_sf, &filter_api).await?;
                }
            } else {
                filter_api.create(&PostParams::default(), &sf.clone().into()).await?;
            }
        }

        Ok(())
    }

    pub async fn update_sgfilter_vec(&self, sgfilters: Vec<&SgSingeFilter>) -> BoxResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = self.get_namespace_api();

            let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                filter_list
            } else {
                let filter_list = filter_api.list(&ListParams::default()).await?;
                filter_map.insert(sf.namespace.clone(), filter_list);
                filter_map.get(&sf.namespace).expect("")
            };

            if let Some(mut old_sf) = namespace_filter.items.iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)).cloned() {
                if old_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                    if let Some(old_filter) = old_sf.spec.filters.iter_mut().find(|qsf| qsf.code == sf.filter.code) {
                        if old_filter.name != sf.filter.name && old_filter.config != sf.filter.config {
                            let _ = mem::replace(
                                old_filter,
                                K8sSgFilterSpecFilter {
                                    code: sf.filter.code.clone(),
                                    name: sf.filter.name.clone(),
                                    enable: true,
                                    config: sf.filter.config.clone(),
                                },
                            );
                            Self::replace_filter(old_sf, &filter_api).await?;
                        }
                    }
                }
            } else {
                filter_api.create(&PostParams::default(), &sf.clone().into()).await?;
            }
        }

        Ok(())
    }

    pub async fn delete_sgfilter_vec(&self, sgfilters: Vec<&SgSingeFilter>) -> BoxResult<()> {
        let mut sg_filter_ns_map = HashMap::new();

        for sf in sgfilters {
            let filter_api: Api<SgFilter> = self.get_namespace_api();

            let namespace_filter = if let Some(filter_list) = sg_filter_ns_map.get(&sf.namespace) {
                filter_list
            } else {
                let filter_list = filter_api.list(&ListParams::default()).await?;
                sg_filter_ns_map.insert(sf.namespace.clone(), filter_list);
                sg_filter_ns_map.get(&sf.namespace).expect("")
            };

            if let Some(mut raw_sf) = namespace_filter.items.iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)).cloned() {
                if raw_sf.spec.target_refs.iter().any(|t_r| *t_r == sf.target_ref) {
                    raw_sf.spec.target_refs.retain(|t_r| *t_r != sf.target_ref);
                    if raw_sf.spec.target_refs.is_empty() {
                        filter_api.delete(&raw_sf.name_any(), &DeleteParams::default()).await?;
                    } else {
                        Self::replace_filter(raw_sf, &filter_api).await?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn replace_filter(mut replace: SgFilter, filter_api: &Api<SgFilter>) -> BoxResult<()> {
        replace.metadata.resource_version = filter_api.get_metadata(replace.name_any().as_str()).await?.metadata.resource_version;
        filter_api.replace(&replace.name_any(), &PostParams::default(), &replace).await?;
        Ok(())
    }
}
