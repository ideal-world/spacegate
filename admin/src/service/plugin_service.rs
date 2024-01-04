use crate::model::query_dto::{GatewayQueryDto, HttpRouteQueryDto, PluginQueryInst, ToInstance as _};
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo::plugin_vo::SgFilterVo;

use crate::service::base_service::VoBaseService;
use kernel_common::client::k8s_client;
use kernel_common::{
    constants::k8s_constants::DEFAULT_NAMESPACE, converter::plugin_k8s_conv::SgSingeFilter, helper::k8s_helper::WarpKubeResult, k8s_crd::sg_filter::K8sSgFilterSpecFilter,
    k8s_crd::sg_filter::SgFilter,
};
use kube::api::{ListParams, PostParams};
use kube::{Api, ResourceExt};
use std::collections::{HashMap, HashSet};
use std::mem;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

use super::gateway_service::GatewayVoService;
use super::route_service::HttpRouteVoService;
use super::spacegate_manage_service::SpacegateManageService;

pub struct PluginVoService;
pub struct PluginK8sService;

impl VoBaseService<SgFilterVo> for PluginVoService {}

impl PluginVoService {
    pub(crate) async fn list(client_name: &str, query: PluginQueryInst) -> TardisResult<Vec<SgFilterVo>> {
        let map = Self::get_type_map(client_name).await?;
        if query.ids.is_none() && query.namespace.is_none() && query.code.is_none() && query.target_kind.is_none() && query.target_name.is_none() && query.target_kind.is_none() {
            Ok(map.into_values().collect())
        } else {
            Ok(map
                .into_values()
                .filter(|f| {
                    query.ids.as_ref().map_or(true, |ids| ids.iter().any(|id| id.is_match(&f.id)))
                        && query.name.as_ref().map_or(true, |name| f.name.as_ref().map_or(false, |f_name| name.is_match(f_name)))
                        && query.code.as_ref().map_or(true, |code| code.is_match(&f.code))
                })
                .collect::<Vec<SgFilterVo>>())
        }
    }

    pub(crate) async fn add(client_name: &str, add: SgFilterVo) -> TardisResult<SgFilterVo> {
        Self::add_vo(client_name, add).await
    }

    pub(crate) async fn update(client_name: &str, update: SgFilterVo) -> TardisResult<SgFilterVo> {
        let id = update.id.clone();
        let result = Self::update_vo(client_name, update).await?;

        // update parent gateway
        join_all(Self::get_ref_gateway(client_name, &id).await?.iter().map(|gateway| GatewayVoService::update(client_name, gateway.clone())).collect::<Vec<_>>())
            .await
            .into_iter()
            .collect::<TardisResult<Vec<_>>>()?;

        // update parent httproute
        join_all(Self::get_ref_httproute(client_name, &id).await?.iter().map(|route| HttpRouteVoService::update(client_name, route.clone())).collect::<Vec<_>>())
            .await
            .into_iter()
            .collect::<TardisResult<Vec<_>>>()?;

        Ok(result)
    }

    pub(crate) async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        let ref_gateways = Self::get_ref_gateway(client_name, id).await?;
        if !ref_gateways.is_empty() {
            return Err(TardisError::bad_request(
                &format!(
                    "[admin.service] {id} is referenced by gateway(s): {}",
                    ref_gateways.iter().map(|x| x.name.as_ref()).collect::<Vec<&str>>().join(",")
                ),
                "",
            ));
        }
        let ref_routes = Self::get_ref_httproute(client_name, id).await?;
        if !ref_routes.is_empty() {
            return Err(TardisError::bad_request(
                &format!(
                    "[admin.service] {id} is referenced by httproute(s): {}",
                    ref_routes.iter().map(|x| x.name.as_ref()).collect::<Vec<&str>>().join(",")
                ),
                "",
            ));
        }

        Self::delete_vo(client_name, id).await?;
        Ok(())
    }

    async fn get_ref_gateway(client_name: &str, id: &str) -> TardisResult<Vec<SgGatewayVo>> {
        GatewayVoService::list(
            client_name,
            GatewayQueryDto {
                filter_ids: Some(vec![id.to_string()]),
                ..Default::default()
            }
            .to_instance()?,
        )
        .await
    }

    async fn get_ref_httproute(client_name: &str, id: &str) -> TardisResult<Vec<SgHttpRouteVo>> {
        HttpRouteVoService::list(
            client_name,
            HttpRouteQueryDto {
                names: None,
                gateway_name: None,
                hostnames: None,
                backend_ids: None,
                filter_ids: Some(vec![id.to_string()]),
            }
            .to_instance()?,
        )
        .await
    }
}

impl PluginK8sService {
    pub(crate) async fn update_filter_changes(client_name: &str, old: Vec<SgSingeFilter>, update: Vec<SgSingeFilter>) -> TardisResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().collect();
        let update_set: HashSet<_> = update.into_iter().collect();

        let update_vec: Vec<_> = old_set.intersection(&update_set).collect();
        PluginK8sService::update_sgfilter_vec(client_name, &update_vec).await?;
        let add_vec: Vec<_> = update_set.difference(&old_set).collect();
        PluginK8sService::add_sgfilter_vec(client_name, &add_vec).await?;
        let delete_vec: Vec<_> = old_set.difference(&update_set).collect();
        PluginK8sService::delete_sgfilter_vec(client_name, &delete_vec).await?;

        Ok(())
    }

    pub async fn add_sgfilter_vec(client_name: &str, sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let mut filter_map = HashMap::new();
            for sf in sgfilters {
                let filter_api: Api<SgFilter> = Self::get_filter_api(client_name, &Some(sf.namespace.clone())).await?;

                let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                    filter_list
                } else {
                    let filter_list = filter_api.list(&ListParams::default()).await.warp_result_by_method("add_sgfilter list")?;
                    filter_map.insert(sf.namespace.clone(), filter_list);
                    filter_map.get(&sf.namespace).expect("")
                };

                if let Some(mut query_sf) = namespace_filter.items.clone().into_iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)) {
                    if !query_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                        query_sf.spec.target_refs.push(sf.target_ref.clone());
                        Self::replace_filter(query_sf, &filter_api, "add sgfilter replace").await?;
                    }
                } else {
                    filter_api.create(&PostParams::default(), &sf.to_sg_filter()).await.warp_result_by_method("add_sgfilter create")?;
                }
            }
        }
        Ok(())
    }

    pub async fn update_sgfilter_vec(client_name: &str, sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        let mut filter_map = HashMap::new();
        if is_kube {
            for sf in sgfilters {
                let filter_api: Api<SgFilter> = Self::get_filter_api(client_name, &Some(sf.namespace.clone())).await?;

                let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                    filter_list
                } else {
                    let filter_list = filter_api.list(&ListParams::default()).await.warp_result_by_method("update_sgfilter list")?;
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
                                Self::replace_filter(old_sf, &filter_api, "update sgfilter replace").await?;
                            }
                        }
                    }
                } else {
                    filter_api.create(&PostParams::default(), &sf.to_sg_filter()).await.warp_result_by_method("update_sgfilter create")?;
                }
            }
        }
        Ok(())
    }

    pub async fn delete_sgfilter_vec(client_name: &str, sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        let mut filter_map = HashMap::new();
        if is_kube {
            for sf in sgfilters {
                let filter_api: Api<SgFilter> = Self::get_filter_api(client_name, &Some(sf.namespace.clone())).await?;

                let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                    filter_list
                } else {
                    let filter_list = filter_api.list(&ListParams::default()).await.warp_result_by_method("delete_sgfilter list")?;
                    filter_map.insert(sf.namespace.clone(), filter_list);
                    filter_map.get(&sf.namespace).expect("")
                };

                if let Some(mut old_sf) = namespace_filter.items.iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)).cloned() {
                    if old_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                        old_sf.spec.target_refs.retain(|t_r| t_r != &sf.target_ref);
                        Self::replace_filter(old_sf, &filter_api, "delete sgfilter replace").await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn replace_filter(mut replace: SgFilter, filter_api: &Api<SgFilter>, fun_str: &str) -> TardisResult<()> {
        replace.metadata.resource_version =
            filter_api.get_metadata(replace.name_any().as_str()).await.warp_result_by_method(&format!("Get Metadata Before {fun_str}"))?.metadata.resource_version;
        filter_api.replace(&replace.name_any(), &PostParams::default(), &replace).await.warp_result_by_method(fun_str)?;
        Ok(())
    }

    #[inline]
    pub async fn get_filter_api(client_name: &str, namespace: &Option<String>) -> TardisResult<Api<SgFilter>> {
        Ok(Api::namespaced(
            (*k8s_client::get(client_name).await?).clone(),
            namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()),
        ))
    }
}
