use crate::helper::get_k8s_client;
use crate::model::query_dto::HttpRouteQueryInst;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::plugin_service::PluginK8sService;

use k8s_openapi::api::core::v1::Secret;
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
#[cfg(feature = "k8s")]
use kernel_common::{
    helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult},
    k8s_crd::http_spaceroute::HttpSpaceroute,
};
#[cfg(feature = "k8s")]
use kube::api::{DeleteParams, PostParams};
#[cfg(feature = "k8s")]
use kube::Api;
use tardis::basic::result::TardisResult;

pub struct HttpRouteVoService;

impl VoBaseService<SgHttpRouteVo> for HttpRouteVoService {}

impl HttpRouteVoService {
    pub(crate) async fn list(query: HttpRouteQueryInst) -> TardisResult<Vec<SgHttpRouteVo>> {
        let map = Self::get_type_map().await?;
        Ok(
            if query.names.is_none() && query.gateway_name.is_none() && query.hostnames.is_none() && query.filter_ids.is_none() {
                map.into_values().collect()
            } else {
                map.into_values()
                    .filter(|r| {
                        query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&r.name)))
                            && query.gateway_name.as_ref().map_or(true, |gateway_name| gateway_name.is_match(&r.gateway_name))
                            && query.hostnames.as_ref().map_or(true, |hostnames| {
                                r.hostnames.as_ref().map_or(false, |r_hostnames| hostnames.iter().any(|hn| r_hostnames.iter().any(|rn| hn.is_match(rn))))
                            })
                            && query.filter_ids.as_ref().map_or(true, |filter_ids| r.filters.iter().any(|f_id| filter_ids.iter().any(|rf| rf.is_match(f_id))))
                    })
                    .collect::<Vec<SgHttpRouteVo>>()
            },
        )
    }

    pub(crate) async fn add(mut add: SgHttpRouteVo) -> TardisResult<SgHttpRouteVo> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone().to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let (httproute, sgfilters) = add_model.to_kube_httproute();
            let http_route_api: Api<HttpSpaceroute> = Api::namespaced(get_k8s_client().await?, &namespace);

            let _ = http_route_api.create(&PostParams::default(), &httproute).await.warp_result_by_method("Add HttpSpaceroute")?;

            PluginK8sService::add_sgfilter_vec(&sgfilters.iter().collect::<Vec<_>>()).await?
        }
        Self::add_vo(add).await
    }
    pub(crate) async fn update(update: SgHttpRouteVo) -> TardisResult<SgHttpRouteVo> {
        let update_un = &update.get_unique_name();

        let update_sg_httproute = update.clone().to_model().await?;
        let old_sg_httproute = Self::get_by_id(&update.name).await?.to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(update_un);
            let http_route_api: Api<HttpSpaceroute> = Api::namespaced(get_k8s_client().await?, &namespace);
            let (update_httproute, update_filter) = update_sg_httproute.to_kube_httproute();
            http_route_api.replace(&name, &PostParams::default(), &update_httproute).await.warp_result_by_method("Replace HttpSpaceroute")?;

            PluginK8sService::update_filter_changes(old_sg_httproute.to_kube_httproute().1, update_filter).await?;
        }
        Self::update_vo(update).await
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        #[cfg(feature = "k8s")]
        {
            let http_route_api: Api<HttpSpaceroute> = Self::get_spaceroute_api(&Some(namespace)).await?;

            http_route_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete HttpSpaceroute")?;

            let old_sg_httproute = Self::get_by_id(id).await?.to_model().await?;
            let (_, f_v) = old_sg_httproute.to_kube_httproute();
            PluginK8sService::delete_sgfilter_vec(&f_v.iter().collect::<Vec<_>>()).await?;
        }
        Self::delete_vo(id).await?;
        Ok(())
    }

    #[cfg(feature = "k8s")]
    #[inline]
    async fn get_spaceroute_api(namespace: &Option<String>) -> TardisResult<Api<HttpSpaceroute>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
