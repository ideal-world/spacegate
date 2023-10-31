use crate::model::query_dto::SgTlsConfigQueryVO;
use crate::model::vo::tls_vo::SgTlsVO;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use k8s_openapi::api::core::v1::Secret;
use kernel_common::helper::k8s_helper::{get_k8s_client, parse_k8s_obj_unique, WarpKubeResult};
use kube::api::{DeleteParams, PostParams};
use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct TlsConfigVoService;

impl VoBaseService<SgTlsVO> for TlsConfigVoService {}

impl TlsConfigVoService {
    pub(crate) async fn list(query: SgTlsConfigQueryVO) -> TardisResult<Vec<SgTlsVO>> {
        //todo query
        Self::get_str_type_map()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgTlsVO>>>()
    }

    pub(crate) async fn add(add: SgTlsVO) -> TardisResult<()> {
        let add_model = add.clone().to_model().await?;
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

    pub(crate) async fn update(update: SgTlsVO) -> TardisResult<()> {
        let unique_name = update.get_unique_name();
        let update_model = update.clone().to_model().await?;
        if let Some(old_str) = Self::get_str_type_map().await?.remove(&unique_name) {
            let mut o: SgTlsVO = serde_json::from_str(&old_str)
                .map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{} failed:{e}", SgTlsVO::get_vo_type(), unique_name), ""))?;
            #[cfg(feature = "k8s")]
            {
                let (namespace, name) = parse_k8s_obj_unique(&unique_name);
                let secret_api: Api<Secret> = Api::namespaced(get_k8s_client().await?, &namespace);
                let s = update_model.to_kube_tls();
                secret_api.replace(&name, &PostParams::default(), &s).await.warp_result_by_method(&format!("Update Secret"))?;
            }
            Self::update_vo(update).await?;
            Ok(())
        } else {
            //todo
            Err(TardisError::not_found("", ""))
        }
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(id);
            let secret_api: Api<Secret> = Api::namespaced(get_k8s_client().await?, &namespace);
            secret_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method(&format!("Delete Secret"))?;
        }
        Self::delete_vo(&id).await?;
        Ok(())
    }
}
