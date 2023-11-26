use crate::model::query_dto::GatewayQueryInst;
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::Vo;
use k8s_gateway_api::Gateway;
use kernel_common::client::{cache_client, k8s_client};
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
use kube::api::{DeleteParams, PostParams};

use kube::Api;
use tardis::TardisFuns;

use super::base_service::VoBaseService;
use super::spacegate_manage_service::SpacegateManageService;
use crate::model::vo_converter::VoConv;
use crate::service::plugin_service::PluginK8sService;
use kernel_common::helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult};
use tardis::basic::result::TardisResult;

pub struct GatewayVoService;

impl VoBaseService<SgGatewayVo> for GatewayVoService {}

impl GatewayVoService {
    pub async fn list(client_name: &str, query: GatewayQueryInst) -> TardisResult<Vec<SgGatewayVo>> {
        Ok(Self::get_type_map(client_name,).await?.into_values().filter(|g|
            if let Some(q_name) = &query.names { q_name.iter().any(|q|q.is_match(&g.name)) } else { true }
                && if let Some(q_port) = &query.port { g.listeners.iter().any(|l| l.port.eq( q_port)) } else { true }
                && if let Some(q_hostname) = &query.hostname {
                g.listeners.iter().any(|l| if let Some(l_hostname)=&l.hostname{q_hostname.is_match(l_hostname)}else { false })
            } else { true })
            .collect())
    }

    pub async fn add(client_name: &str, mut add: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone().to_model(client_name).await?;
        if is_kube {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let (gateway, sgfilters) = add_model.to_kube_gateway();

            let gateway_api: Api<Gateway> = Self::get_gateway_api(client_name, &Some(namespace)).await?;

            let _ = gateway_api.create(&PostParams::default(), &gateway).await.warp_result_by_method("Add Gateway")?;

            PluginK8sService::add_sgfilter_vec(client_name, &sgfilters.iter().collect::<Vec<_>>()).await?
        } else {
            cache_client::add_or_update_obj(
                client_name,
                cache_client::CONF_GATEWAY_KEY,
                &add_model.name,
                &add_model.name,
                &TardisFuns::json.obj_to_string(&add_model)?,
            )
            .await?
        }
        Self::add_vo(client_name, add).await
    }

    pub async fn update_by_id(client_name: &str, id: &str) -> TardisResult<SgGatewayVo> {
        let gateway_o = Self::get_by_id(client_name, id).await?;
        GatewayVoService::update(client_name, gateway_o).await
    }

    pub async fn update(client_name: &str, update: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        let update_un = &update.get_unique_name();

        let update_sg_gateway = update.clone().to_model(client_name).await?;
        let old_sg_gateway = Self::get_by_id(client_name, &update.name).await?.to_model(client_name).await?;
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, name) = parse_k8s_obj_unique(update_un);
            let gateway_api: Api<Gateway> = Self::get_gateway_api(client_name, &Some(namespace)).await?;
            let (update_gateway, update_filter) = update_sg_gateway.to_kube_gateway();
            gateway_api.replace(&name, &PostParams::default(), &update_gateway).await.warp_result_by_method("Replace Gateway")?;

            PluginK8sService::update_filter_changes(client_name, old_sg_gateway.to_kube_gateway().1, update_filter).await?;
        } else {
            cache_client::add_or_update_obj(
                client_name,
                cache_client::CONF_GATEWAY_KEY,
                &update_sg_gateway.name,
                &update_sg_gateway.name,
                &TardisFuns::json.obj_to_string(&update_sg_gateway)?,
            )
            .await?
        }
        Self::update_vo(client_name, update).await
    }

    pub async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let gateway_api: Api<Gateway> = Self::get_gateway_api(client_name, &Some(namespace)).await?;

            gateway_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Gateway")?;

            let old_sg_gateway = Self::get_by_id(client_name, id).await?.to_model(client_name).await?;
            let (_, f_v) = old_sg_gateway.to_kube_gateway();
            PluginK8sService::delete_sgfilter_vec(client_name, &f_v.iter().collect::<Vec<_>>()).await?;
        } else {
            cache_client::delete_obj(client_name, cache_client::CONF_GATEWAY_KEY, id, id).await?;
        }
        Self::delete_vo(client_name, id).await?;

        Ok(())
    }

    async fn get_gateway_api(client_name: &str, namespace: &Option<String>) -> TardisResult<Api<Gateway>> {
        Ok(Api::namespaced(
            (*k8s_client::get(client_name).await?).clone(),
            namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()),
        ))
    }
}
