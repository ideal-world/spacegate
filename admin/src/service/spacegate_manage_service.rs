use crate::client::get_base_is_kube;
use crate::constants::{KUBE_VO_NAMESPACE, TYPE_CONFIG_NAME_MAP};
use crate::model::query_dto::{SpacegateInstQueryDto, SpacegateInstQueryInst, ToInstance};
use crate::model::vo::spacegate_inst_vo::{InstConfigType, InstConfigVo};
use crate::model::vo::Vo;
use crate::service::base_service::VoBaseService;

use k8s_openapi::api::core::v1::ConfigMap;
use kernel_common::client::k8s_client;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;

use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::TardisFuns;

pub struct SpacegateManageService;

impl VoBaseService<InstConfigVo> for SpacegateManageService {}

impl SpacegateManageService {
    pub(crate) async fn list(query: SpacegateInstQueryInst) -> TardisResult<Vec<InstConfigVo>> {
        let map = Self::get_type_map(DEFAULT_CLIENT_NAME).await?;
        if query.names.is_none() {
            Ok(map.into_values().collect())
        } else {
            Ok(map
                .into_values()
                .filter(|inst_config| query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&inst_config.get_unique_name()))))
                .collect::<Vec<InstConfigVo>>())
        }
    }

    pub(crate) async fn add(add: InstConfigVo) -> TardisResult<InstConfigVo> {
        if add.get_unique_name() == DEFAULT_CLIENT_NAME || add.get_unique_name().is_empty() {
            return Err(TardisError::bad_request(&format!("[admin.service] client name {DEFAULT_CLIENT_NAME} is not allowed"), ""));
        }
        Self::add_vo(DEFAULT_CLIENT_NAME, add).await
    }

    pub(crate) async fn update(update: InstConfigVo) -> TardisResult<InstConfigVo> {
        if update.get_unique_name() == DEFAULT_CLIENT_NAME || update.get_unique_name().is_empty() {
            return Err(TardisError::bad_request(&format!("[admin.service] client name {DEFAULT_CLIENT_NAME} is not allowed"), ""));
        }
        let unique_name = update.get_unique_name();
        if let Some(_old_str) = Self::get_str_type_map(DEFAULT_CLIENT_NAME).await?.remove(&unique_name) {
            Self::update_vo(DEFAULT_CLIENT_NAME, update).await
        } else {
            Err(TardisError::not_found(&format!("[admin.service] Update tls {} not found", unique_name), ""))
        }
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(DEFAULT_CLIENT_NAME, id).await?;
        Ok(())
    }

    pub(crate) async fn check(id: &str) -> TardisResult<()> {
        if Self::list(
            SpacegateInstQueryDto {
                names: Some(vec![id.to_string()]),
            }
            .to_instance()?,
        )
        .await?
        .is_empty()
        {
            return Err(TardisError::bad_request(&format!("[admin.service] spacegate inst [{}] not found", id), ""));
        };
        Ok(())
    }

    pub async fn client_is_kube(name: &str) -> TardisResult<bool> {
        if name == DEFAULT_CLIENT_NAME {
            get_base_is_kube().await
        } else {
            let config_str = if get_base_is_kube().await? {
                let api: Api<ConfigMap> = Api::namespaced((*k8s_client::get(DEFAULT_CLIENT_NAME).await?).clone(), KUBE_VO_NAMESPACE);

                api.get_opt(TYPE_CONFIG_NAME_MAP.get(InstConfigVo::get_vo_type().as_str()).expect(""))
                    .await
                    .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error: {e}"), ""))?
                    .ok_or(TardisError::wrap("[SG.admin] Kubernetes client error", ""))?
                    .data
                    .ok_or(TardisError::wrap("[SG.admin] Kubernetes client error", ""))?
                    .remove(name)
                    .ok_or(TardisError::wrap("[SG.admin] Kubernetes client error", ""))?
            } else {
                TardisFuns::cache()
                    .hget(TYPE_CONFIG_NAME_MAP.get(InstConfigVo::get_vo_type().as_str()).expect(""), name)
                    .await?
                    .ok_or(TardisError::wrap("[SG.admin] Kubernetes client error", ""))?
            };
            let config = TardisFuns::json.str_to_obj::<InstConfigVo>(&config_str)?;
            Ok(config.type_ == InstConfigType::K8sClusterConfig)
        }
    }
}
