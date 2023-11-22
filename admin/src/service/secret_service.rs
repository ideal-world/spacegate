use crate::helper::get_k8s_client;
use crate::model::query_dto::{GatewayQueryDto, SgTlsQueryInst, ToInstance};
use crate::model::vo::Vo;
use crate::service::base_service::VoBaseService;
use crate::service::gateway_service::GatewayVoService;
use k8s_openapi::api::core::v1::Secret;
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
use kernel_common::{
    helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult},
    inner_model::gateway::SgTls,
};
use kube::api::{DeleteParams, PostParams};
use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct TlsVoService;

impl VoBaseService<SgTls> for TlsVoService {}

impl TlsVoService {
    pub(crate) async fn list(clinet_name: &str, query: SgTlsQueryInst) -> TardisResult<Vec<SgTls>> {
        let map = Self::get_type_map(clinet_name).await?;
        if query.names.is_none() {
            Ok(map.into_values().collect())
        } else {
            Ok(map.into_values().filter(|t| query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&t.name)))).collect::<Vec<SgTls>>())
        }
    }

    pub(crate) async fn add(clinet_name: &str, mut add: SgTls) -> TardisResult<()> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone();
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let secret_api: Api<Secret> = Self::get_secret_api(&Some(namespace)).await?;
            let s = add_model.to_kube_tls();
            secret_api.create(&PostParams::default(), &s).await.warp_result_by_method("Add Secret")?;
        }
        Self::add_vo(clinet_name, add).await?;
        Ok(())
    }

    pub(crate) async fn update(clinet_name: &str, update: SgTls) -> TardisResult<()> {
        let unique_name = update.get_unique_name();
        if let Some(_old_str) = Self::get_str_type_map(clinet_name).await?.remove(&unique_name) {
            #[cfg(feature = "k8s")]
            {
                let (namespace, name) = parse_k8s_obj_unique(&unique_name);
                let secret_api: Api<Secret> = Self::get_secret_api(&Some(namespace)).await?;
                let s = update.clone().to_kube_tls();
                secret_api.replace(&name, &PostParams::default(), &s).await.warp_result_by_method("Update Secret")?;
            }
            Self::update_vo(clinet_name, update).await?;
            Ok(())
        } else {
            Err(TardisError::not_found(&format!("[admin.service] Update tls {} not found", unique_name), ""))
        }
    }

    pub(crate) async fn delete(clinet_name: &str, id: &str) -> TardisResult<()> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(id);
            let secret_api: Api<Secret> = Self::get_secret_api(&Some(namespace)).await?;
            secret_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Secret")?;
        }
        let gateways = GatewayVoService::list(clinet_name, GatewayQueryDto { ..Default::default() }.to_instance()?).await?;
        if gateways.is_empty() {
            Self::delete_vo(clinet_name, id).await?;
            Ok(())
        } else {
            Err(TardisError::bad_request(
                &format!(
                    "[admin.service] Delete tls {id} is used by gateway:{}",
                    gateways.iter().map(|g| g.get_unique_name()).collect::<Vec<String>>().join(",")
                ),
                "",
            ))
        }
    }

    async fn get_secret_api(clinet_name: &str, namespace: &Option<String>) -> TardisResult<Api<Secret>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
