use crate::model::query_dto::{GatewayQueryDto, GatewayQueryInst, SgTlsQueryInst, SgTlsQueryVO, ToInstance};
use crate::model::vo::Vo;
use crate::service::base_service::VoBaseService;
use crate::service::gateway_service::GatewayVoService;
use k8s_openapi::api::core::v1::Secret;
use kernel_common::helper::k8s_helper::{get_k8s_client, parse_k8s_obj_unique, WarpKubeResult};
use kernel_common::inner_model::gateway::SgTls;
use kube::api::{DeleteParams, PostParams};
use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct TlsVoService;

impl VoBaseService<SgTls> for TlsVoService {}

impl TlsVoService {
    pub(crate) async fn list(query: SgTlsQueryInst) -> TardisResult<Vec<SgTls>> {
        let map = Self::get_type_map().await?;
        if query.names.is_none() {
            Ok(map.into_values().collect())
        } else {
            Ok(map.into_values().into_iter().filter(|t| query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&t.name)))).collect::<Vec<SgTls>>())
        }
    }

    pub(crate) async fn add(add: SgTls) -> TardisResult<()> {
        let add_model = add.clone();
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_obj_unique(&add.get_unique_name());
            let secret_api: Api<Secret> = Api::namespaced(get_k8s_client().await?, &namespace);
            let s = add_model.to_kube_tls();
            secret_api.create(&PostParams::default(), &s).await.warp_result_by_method(&format!("Add Secret"))?;
        }
        Self::add_vo(add).await?;
        Ok(())
    }

    pub(crate) async fn update(update: SgTls) -> TardisResult<()> {
        let unique_name = update.get_unique_name();
        if let Some(old_str) = Self::get_str_type_map().await?.remove(&unique_name) {
            // let mut o: SgTls = serde_json::from_str(&old_str)
            //     .map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{} failed:{e}", SgTls::get_vo_type(), unique_name), ""))?;
            #[cfg(feature = "k8s")]
            {
                let (namespace, name) = parse_k8s_obj_unique(&unique_name);
                let secret_api: Api<Secret> = Api::namespaced(get_k8s_client().await?, &namespace);
                let s = update.clone().to_kube_tls();
                secret_api.replace(&name, &PostParams::default(), &s).await.warp_result_by_method(&format!("Update Secret"))?;
            }
            Self::update_vo(update).await?;
            Ok(())
        } else {
            Err(TardisError::not_found(&format!("[admin.service] Update tls {} not found", unique_name), ""))
        }
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(id);
            let secret_api: Api<Secret> = Api::namespaced(get_k8s_client().await?, &namespace);
            secret_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method(&format!("Delete Secret"))?;
        }
        let gateways = GatewayVoService::list(GatewayQueryDto { ..Default::default() }.to_instance()?).await?;
        if gateways.is_empty() {
            Self::delete_vo(&id).await?;
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
}
