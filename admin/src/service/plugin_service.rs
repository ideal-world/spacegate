use crate::dto::query_dto::PluginQueryDto;
use crate::dto::ToFields;
#[cfg(feature = "k8s")]
use crate::service::helper::get_k8s_client;
use crate::service::helper::WarpKubeResult;
use kernel_dto::constants::DEFAULT_NAMESPACE;
use kernel_dto::dto::plugin_filter_dto::{SgRouteFilter, SgSingeFilter};
#[cfg(feature = "k8s")]
use kernel_dto::k8s_crd::SgFilter;
use kube::api::PostParams;
use kube::ResourceExt;
#[cfg(feature = "k8s")]
use kube::{api::ListParams, Api};
use std::collections::HashSet;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct PluginService;

impl PluginService {
    pub async fn list(query: PluginQueryDto) -> TardisResult<Vec<SgRouteFilter>> {
        let result = vec![];
        #[cfg(feature = "k8s")]
        {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&query.namespace).await?;

            let filter_list = filter_api.list(&ListParams::default().fields(&query.to_fields())).await.map_err(|e| TardisError::io_error(&format!("err:{e}"), ""))?;
        }
        #[cfg(not(feature = "k8s"))]
        {}

        Ok(result)
    }

    #[cfg(feature = "k8s")]
    pub async fn add_sgfilter_vec(sgfilters: Vec<SgSingeFilter>) -> TardisResult<Vec<SgFilter>> {
        let add_filter = sgfilters.iter().map(|f| f.spec.filters.iter().map(|f_f| (f_f.name.clone(), f_f.code.clone())).collect::<Vec<_>>()).flatten().collect::<HashSet<_>>();
        let add_target = sgfilters
            .iter()
            .map(|f| {let f_t=f.target_refs.clone(); (f_t.name.clone(), f_t.namespace.clone(), f_t.kind.clone())}).collect::<Vec<_>>()
            .flatten()
            .collect::<HashSet<_>>();
        for sf in sgfilters {
            let filter_api: Api<SgFilter> = Self::get_filter_api(&sf.namespace).await?;
            if let Some(mut query_sf) = filter_api.get_opt(&sf.name_any()).await.warp_result_by_method("get")? {
                sf.spec.filters.iter().filter(|&x| query_sf.spec.filters.iter().any(|qsf| qsf.code == x.code)).collect();
                filter_api.replace(&query_sf.name_any(), &PostParams::default(), &query_sf).await.warp_result_by_method("replace")?;
            } else {
                filter_api.create(&PostParams::default(), &sf).await.warp_result_by_method("create")?;
            }
        }

        Ok(sgfilters)
    }

    #[cfg(feature = "k8s")]
    #[inline]
    pub async fn get_filter_api(namespace: &Option<String>) -> TardisResult<Api<SgFilter>> {
        Ok(Api::namespaced(get_k8s_client().await?, &namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
