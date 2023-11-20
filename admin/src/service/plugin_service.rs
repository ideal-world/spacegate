#[cfg(feature = "k8s")]
use crate::helper::get_k8s_client;
use crate::model::query_dto::PluginQueryInst;
use crate::model::vo::plugin_vo::SgFilterVo;

use crate::service::base_service::VoBaseService;
#[cfg(feature = "k8s")]
use kernel_common::{
    constants::k8s_constants::DEFAULT_NAMESPACE, converter::plugin_k8s_conv::SgSingeFilter, helper::k8s_helper::WarpKubeResult, k8s_crd::sg_filter::K8sSgFilterSpecFilter,
    k8s_crd::sg_filter::SgFilter,
};
#[cfg(feature = "k8s")]
use kube::api::{ListParams, PostParams};
#[cfg(feature = "k8s")]
use kube::{Api, ResourceExt};
use std::collections::{HashMap, HashSet};
use tardis::basic::result::TardisResult;

pub struct PluginVoService;
#[cfg(feature = "k8s")]
pub struct PluginK8sService;

impl VoBaseService<SgFilterVo> for PluginVoService {}

impl PluginVoService {
    pub(crate) async fn list(query: PluginQueryInst) -> TardisResult<Vec<SgFilterVo>> {
        let map = Self::get_type_map().await?;
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

    pub(crate) async fn add(add: SgFilterVo) -> TardisResult<SgFilterVo> {
        Self::add_vo(add).await
    }

    pub(crate) async fn update(update: SgFilterVo) -> TardisResult<SgFilterVo> {
        Self::update_vo(update).await
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(id).await?;
        Ok(())
    }
}

#[cfg(feature = "k8s")]
impl PluginK8sService {
    pub(crate) async fn update_filter_changes(old: Vec<SgSingeFilter>, update: Vec<SgSingeFilter>) -> TardisResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().collect();
        let update_set: HashSet<_> = update.into_iter().collect();

        let update_vec: Vec<_> = old_set.intersection(&update_set).collect();
        PluginK8sService::update_sgfilter_vec(&update_vec).await?;
        let add_vec: Vec<_> = update_set.difference(&old_set).collect();
        PluginK8sService::add_sgfilter_vec(&add_vec).await?;
        let delete_vec: Vec<_> = old_set.difference(&update_set).collect();
        PluginK8sService::delete_sgfilter_vec(&delete_vec).await?;

        Ok(())
    }

    pub async fn add_sgfilter_vec(sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&Some(sf.namespace.clone())).await?;

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
                    filter_api.replace(&query_sf.name_any(), &PostParams::default(), &query_sf).await.warp_result_by_method("add_sgfilter replace")?;
                }
            } else {
                filter_api.create(&PostParams::default(), &sf.to_sg_filter()).await.warp_result_by_method("add_sgfilter create")?;
            }
        }

        Ok(())
    }

    pub async fn update_sgfilter_vec(sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&Some(sf.namespace.clone())).await?;

            let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                filter_list
            } else {
                let filter_list = filter_api.list(&ListParams::default()).await.warp_result_by_method("update_sgfilter list")?;
                filter_map.insert(sf.namespace.clone(), filter_list);
                filter_map.get(&sf.namespace).expect("")
            };

            if let Some(old_sf) = namespace_filter.items.iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)).cloned() {
                if old_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                    if let Some(mut old_filter) = old_sf.spec.filters.iter().find(|qsf| qsf.code == sf.filter.code) {
                        if old_filter.name != sf.filter.name && old_filter.config != sf.filter.config {
                            old_filter = &K8sSgFilterSpecFilter {
                                code: sf.filter.code.clone(),
                                name: sf.filter.name.clone(),
                                enable: true,
                                config: sf.filter.config.clone(),
                            };
                            filter_api.replace(&old_sf.name_any(), &PostParams::default(), &old_sf).await.warp_result_by_method("update_sgfilter replace")?;
                        }
                    }
                }
            } else {
                filter_api.create(&PostParams::default(), &sf.to_sg_filter()).await.warp_result_by_method("update_sgfilter create")?;
            }
        }
        Ok(())
    }

    pub async fn delete_sgfilter_vec(sgfilters: &[&SgSingeFilter]) -> TardisResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&Some(sf.namespace.clone())).await?;

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
                    filter_api.replace(&old_sf.name_any(), &PostParams::default(), &old_sf).await.warp_result_by_method("delete_sgfilter replace")?;
                }
            }
        }
        Ok(())
    }

    #[inline]
    pub async fn get_filter_api(namespace: &Option<String>) -> TardisResult<Api<SgFilter>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
