#[cfg(feature = "k8s")]
use crate::helper::get_k8s_client;
use crate::model::query_dto::GatewayQueryInst;
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::Vo;
#[cfg(feature = "k8s")]
use k8s_gateway_api::Gateway;
#[cfg(feature = "k8s")]
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
#[cfg(feature = "k8s")]
use kube::api::{DeleteParams, PostParams};

#[cfg(feature = "k8s")]
use kube::Api;

use super::base_service::VoBaseService;
use crate::model::vo_converter::VoConv;
use crate::service::plugin_service::PluginK8sService;
#[cfg(feature = "k8s")]
use kernel_common::helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult};
use tardis::basic::result::TardisResult;

pub struct GatewayVoService;

impl VoBaseService<SgGatewayVo> for GatewayVoService {}

impl GatewayVoService {
    pub async fn list(query: GatewayQueryInst) -> TardisResult<Vec<SgGatewayVo>> {
        Ok(Self::get_type_map().await?.into_values().filter(|g|
            if let Some(q_name) = &query.names { q_name.iter().any(|q|q.is_match(&g.name)) } else { true }
                && if let Some(q_port) = &query.port { g.listeners.iter().any(|l| l.port.eq( q_port)) } else { true }
                && if let Some(q_hostname) = &query.hostname {
                g.listeners.iter().any(|l| if let Some(l_hostname)=&l.hostname{q_hostname.is_match(l_hostname)}else { false })
            } else { true })
            .collect())
    }
    pub async fn add(mut add: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone().to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let (gateway, sgfilters) = add_model.to_kube_gateway();

            let gateway_api: Api<Gateway> = Self::get_gateway_api(&Some(namespace)).await?;

            let _ = gateway_api.create(&PostParams::default(), &gateway).await.warp_result_by_method("Add Gateway")?;

            PluginK8sService::add_sgfilter_vec(&sgfilters.iter().collect::<Vec<_>>()).await?
        }
        Self::add_vo(add).await
    }

    pub async fn update_by_id(id: &str) -> TardisResult<SgGatewayVo> {
        let gateway_o = Self::get_by_id(id).await?;
        GatewayVoService::update(gateway_o).await
    }

    pub async fn update(update: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        let update_un = &update.get_unique_name();

        let update_sg_gateway = update.clone().to_model().await?;
        let old_sg_gateway = Self::get_by_id(&update.name).await?.to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(update_un);
            let gateway_api: Api<Gateway> = Self::get_gateway_api(&Some(namespace)).await?;
            let (update_gateway, update_filter) = update_sg_gateway.to_kube_gateway();
            gateway_api.replace(&name, &PostParams::default(), &update_gateway).await.warp_result_by_method("Replace Gateway")?;

            PluginK8sService::update_filter_changes(old_sg_gateway.to_kube_gateway().1, update_filter).await?;
        }
        Self::update_vo(update).await
    }

    pub async fn delete(id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        #[cfg(feature = "k8s")]
        {
            let gateway_api: Api<Gateway> = Self::get_gateway_api(&Some(namespace)).await?;

            gateway_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Gateway")?;

            let old_sg_gateway = Self::get_by_id(id).await?.to_model().await?;
            let (_, f_v) = old_sg_gateway.to_kube_gateway();
            PluginK8sService::delete_sgfilter_vec(&f_v.iter().collect::<Vec<_>>()).await?;
        }
        Self::delete_vo(id).await?;

        Ok(())
    }

    #[cfg(feature = "k8s")]
    async fn get_gateway_api(namespace: &Option<String>) -> TardisResult<Api<Gateway>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
