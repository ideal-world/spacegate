use crate::model::query_dto::HttpRouteQueryInst;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::plugin_service::PluginK8sService;

use kernel_common::client::{cache_client, k8s_client};
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
use kernel_common::{
    helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult},
    k8s_crd::http_spaceroute::HttpSpaceroute,
};
use kube::api::{DeleteParams, PostParams};
use kube::Api;
use tardis::basic::result::TardisResult;
use tardis::TardisFuns;

use super::spacegate_manage_service::SpacegateManageService;

pub struct HttpRouteVoService;

impl VoBaseService<SgHttpRouteVo> for HttpRouteVoService {}

impl HttpRouteVoService {
    pub(crate) async fn list(client_name: &str, query: HttpRouteQueryInst) -> TardisResult<Vec<SgHttpRouteVo>> {
        let map = Self::get_type_map(client_name).await?;
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

    pub(crate) async fn add(client_name: &str, mut add: SgHttpRouteVo) -> TardisResult<SgHttpRouteVo> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone().to_model(client_name).await?;
        if is_kube {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let (httproute, sgfilters) = add_model.to_kube_httproute();
            let http_route_api: Api<HttpSpaceroute> = Self::get_spaceroute_api(client_name, &Some(namespace)).await?;
            let _ = http_route_api.create(&PostParams::default(), &httproute).await.warp_result_by_method("Add HttpSpaceroute")?;

            PluginK8sService::add_sgfilter_vec(client_name, &sgfilters.iter().collect::<Vec<_>>()).await?
        } else {
            cache_client::add_or_update_obj(
                client_name,
                cache_client::CONF_HTTP_ROUTE_KEY,
                &add_model.gateway_name,
                &add_model.name,
                &TardisFuns::json.obj_to_string(&add_model)?,
            )
            .await?
        }
        Self::add_vo(client_name, add).await
    }
    pub(crate) async fn update(client_name: &str, update: SgHttpRouteVo) -> TardisResult<SgHttpRouteVo> {
        let update_un = &update.get_unique_name();

        let update_sg_httproute = update.clone().to_model(client_name).await?;
        let old_sg_httproute = Self::get_by_id(client_name, &update.name).await?.to_model(client_name).await?;
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, name) = parse_k8s_obj_unique(update_un);
            let http_route_api: Api<HttpSpaceroute> = Self::get_spaceroute_api(client_name, &Some(namespace)).await?;
            let (update_httproute, update_filter) = update_sg_httproute.to_kube_httproute();
            http_route_api.replace(&name, &PostParams::default(), &update_httproute).await.warp_result_by_method("Replace HttpSpaceroute")?;

            PluginK8sService::update_filter_changes(client_name, old_sg_httproute.to_kube_httproute().1, update_filter).await?;
        } else {
            cache_client::add_or_update_obj(
                client_name,
                cache_client::CONF_HTTP_ROUTE_KEY,
                &update_sg_httproute.gateway_name,
                &update_sg_httproute.name,
                &TardisFuns::json.obj_to_string(&update_sg_httproute)?,
            )
            .await?
        }
        Self::update_vo(client_name, update).await
    }

    pub(crate) async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let http_route_api: Api<HttpSpaceroute> = Self::get_spaceroute_api(client_name, &Some(namespace)).await?;

            http_route_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete HttpSpaceroute")?;

            let old_sg_httproute = Self::get_by_id(client_name, id).await?.to_model(client_name).await?;
            let (_, f_v) = old_sg_httproute.to_kube_httproute();
            PluginK8sService::delete_sgfilter_vec(client_name, &f_v.iter().collect::<Vec<_>>()).await?;
        } else {
            let old_httproute = Self::get_by_id(client_name, id).await?;
            cache_client::delete_obj(client_name, cache_client::CONF_HTTP_ROUTE_KEY, &old_httproute.gateway_name, id).await?;
        }
        Self::delete_vo(client_name, id).await?;
        Ok(())
    }

    #[inline]
    async fn get_spaceroute_api(client_name: &str, namespace: &Option<String>) -> TardisResult<Api<HttpSpaceroute>> {
        Ok(Api::namespaced(
            (*k8s_client::get(client_name).await?).clone(),
            namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()),
        ))
    }
}
