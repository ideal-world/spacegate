#[cfg(feature = "k8s")]
use crate::helper::{get_k8s_client, WarpKubeResult};
use crate::model::query_dto::{BackendRefQueryDto, PluginQueryDto};
use crate::model::vo::backend_vo::BackendRefVO;
use crate::model::vo::plugin_vo::SgFilterVO;
#[cfg(feature = "k8s")]
use crate::model::ToFields;
use crate::service::base_service::BaseService;
#[cfg(feature = "k8s")]
use kernel_common::constants::DEFAULT_NAMESPACE;
#[cfg(feature = "k8s")]
use kernel_common::converter::plugin_k8s_conv::SgSingeFilter;
use kernel_common::inner_model::plugin_filter::SgRouteFilter;
#[cfg(feature = "k8s")]
use kernel_common::k8s_crd::sg_filter::SgFilter;
#[cfg(feature = "k8s")]
use kube::{api::ListParams, api::PostParams, Api, ResourceExt};
use std::collections::HashMap;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct PluginService;

impl PluginService {
    pub(crate) async fn list(query: PluginQueryDto) -> TardisResult<Vec<SgFilterVO>> {
        //todo query
        BaseService::get_type_map::<SgFilterVO>()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgFilterVO>>>()
    }

    pub(crate) async fn add(add: BackendRefVO) -> TardisResult<()> {
        BaseService::add::<BackendRefVO>(add).await?;
        Ok(())
    }
    pub(crate) async fn update(update: BackendRefVO) -> TardisResult<()> {
        BaseService::update::<BackendRefVO>(update).await?;
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        BaseService::delete::<BackendRefVO>(&id).await?;
        Ok(())
    }

    #[cfg(feature = "k8s")]
    pub async fn add_sgfilter_vec(sgfilters: Vec<SgSingeFilter>) -> TardisResult<()> {
        let mut filter_map = HashMap::new();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&Some(sf.namespace.clone())).await?;

            let namespace_filter = if let Some(filter_list) = filter_map.get(&sf.namespace) {
                filter_list
            } else {
                let filter_list = filter_api.list(&ListParams::default()).await.warp_result_by_method("list")?;
                filter_map.insert(sf.namespace.clone(), filter_list);
                filter_map.get(&sf.namespace).expect("")
            };

            if let Some(mut query_sf) = namespace_filter.items.clone().into_iter().find(|f| f.spec.filters.iter().any(|qsf| qsf.code == sf.filter.code)) {
                if query_sf.spec.target_refs.iter().any(|t_r| t_r == &sf.target_ref) {
                    //存在
                } else {
                    query_sf.spec.target_refs.push(sf.target_ref);
                    filter_api.replace(&query_sf.name_any(), &PostParams::default(), &query_sf).await.warp_result_by_method("replace")?;
                }
            } else {
                filter_api.create(&PostParams::default(), &sf.to_sg_filter()).await.warp_result_by_method("create")?;
            }
        }

        Ok(())
    }

    #[cfg(feature = "k8s")]
    #[inline]
    pub async fn get_filter_api(namespace: &Option<String>) -> TardisResult<Api<SgFilter>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}