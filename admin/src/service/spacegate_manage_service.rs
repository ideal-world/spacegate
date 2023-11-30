use crate::api::SessionInstance;
use crate::client::get_base_is_kube;
use crate::config::k8s_config::ToKubeconfig;
use crate::constants::{KUBE_VO_NAMESPACE, SESSION_INSTACE_KEY, TYPE_CONFIG_NAME_MAP};
use crate::model::query_dto::{SpacegateInstQueryDto, SpacegateInstQueryInst, ToInstance};
use crate::model::vo::spacegate_inst_vo::{InstConfigType, InstConfigVo};
use crate::model::vo::Vo;
use crate::service::base_service::VoBaseService;

use k8s_openapi::api::core::v1::ConfigMap;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use kernel_common::client::{cache_client, k8s_client};

use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use tardis::web::poem::session::Session;
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
        match add.type_ {
            InstConfigType::K8sClusterConfig => {
                if add.k8s_cluster_config.is_none() {
                    return Err(TardisError::bad_request("[admin.service] k8s_cluster_config is required", ""));
                }
            }
            InstConfigType::RedisConfig => {
                if add.redis_config.is_none() {
                    return Err(TardisError::bad_request("[admin.service] redis_config is required", ""));
                }
            }
        }
        let name = add.get_unique_name();
        if name == DEFAULT_CLIENT_NAME || name.is_empty() {
            return Err(TardisError::bad_request(&format!("[admin.service] client name {DEFAULT_CLIENT_NAME} is not allowed"), ""));
        }
        match add.type_ {
            InstConfigType::K8sClusterConfig => {
                let config = add.k8s_cluster_config.clone().expect("").to_kubeconfig();

                k8s_client::inst(name, config).await?;
            }
            InstConfigType::RedisConfig => {
                cache_client::init(name, &add.redis_config.clone().expect("").url).await?;
            }
        }
        Self::add_vo(DEFAULT_CLIENT_NAME, add).await
    }

    pub(crate) async fn update(update: InstConfigVo) -> TardisResult<InstConfigVo> {
        match update.type_ {
            InstConfigType::K8sClusterConfig => {
                if update.k8s_cluster_config.is_none() {
                    return Err(TardisError::bad_request("[admin.service] k8s_cluster_config is required", ""));
                }
            }
            InstConfigType::RedisConfig => {
                if update.redis_config.is_none() {
                    return Err(TardisError::bad_request("[admin.service] redis_config is required", ""));
                }
            }
        }

        if update.get_unique_name() == DEFAULT_CLIENT_NAME || update.get_unique_name().is_empty() {
            return Err(TardisError::bad_request(&format!("[admin.service] client name {DEFAULT_CLIENT_NAME} is not allowed"), ""));
        }
        let unique_name = update.get_unique_name();
        if Self::get_str_type_map(DEFAULT_CLIENT_NAME).await?.remove(&unique_name).is_none() {
            return Err(TardisError::not_found(&format!("[admin.service] Update tls {} not found", unique_name), ""));
        }

        match update.type_ {
            InstConfigType::K8sClusterConfig => {
                let config = update.k8s_cluster_config.clone().expect("").to_kubeconfig();
                k8s_client::inst(unique_name, config).await?;
            }
            InstConfigType::RedisConfig => {
                cache_client::init(unique_name, &update.redis_config.clone().expect("").url).await?;
            }
        }
        Self::update_vo(DEFAULT_CLIENT_NAME, update).await
    }

    pub(crate) async fn delete(name: &str) -> TardisResult<()> {
        if name == DEFAULT_CLIENT_NAME || name.is_empty() {
            return Err(TardisError::bad_request(&format!("[admin.service] client name {DEFAULT_CLIENT_NAME} is not allowed"), ""));
        }
        let old_vo = Self::get_by_id(DEFAULT_CLIENT_NAME, name).await?;
        match old_vo.type_ {
            InstConfigType::K8sClusterConfig => {
                k8s_client::remove(&name).await?;
            }
            InstConfigType::RedisConfig => {
                cache_client::remove(name).await?;
            }
        }
        Self::delete_vo(DEFAULT_CLIENT_NAME, name).await?;
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

    pub async fn get_instance(session: &Session) -> TardisResult<SessionInstance> {
        if let Some(instance_str) = session.get::<String>(SESSION_INSTACE_KEY) {
            if let Ok(instance) = TardisFuns::json.str_to_obj::<SessionInstance>(&instance_str) {
                return Ok(instance);
            }
        }
        SpacegateManageService::set_instance_name(DEFAULT_CLIENT_NAME, session).await
    }

    pub async fn set_instance_name(name: &str, session: &Session) -> TardisResult<SessionInstance> {
        if name.is_empty() {
            return Err(TardisError::bad_request("[admin] select name cannot be empty", ""));
        }
        let session_instace = if name == DEFAULT_CLIENT_NAME {
            SessionInstance {
                name: name.to_string(),
                type_: if get_base_is_kube().await? {
                    InstConfigType::K8sClusterConfig
                } else {
                    InstConfigType::RedisConfig
                },
            }
        } else {
            SpacegateManageService::check(name).await?;
            SessionInstance {
                name: name.to_string(),
                type_: if Self::client_is_kube(name).await? {
                    InstConfigType::K8sClusterConfig
                } else {
                    InstConfigType::RedisConfig
                },
            }
        };
        session.set(SESSION_INSTACE_KEY, TardisFuns::json.obj_to_string(&session_instace)?);
        Ok(session_instace)
    }

    pub async fn client_is_kube(name: &str) -> TardisResult<bool> {
        if name == DEFAULT_CLIENT_NAME {
            get_base_is_kube().await
        } else {
            let config_str = if get_base_is_kube().await? {
                let api: Api<ConfigMap> = Api::namespaced((*k8s_client::get(DEFAULT_CLIENT_NAME).await?).clone(), KUBE_VO_NAMESPACE);

                api.get_opt(TYPE_CONFIG_NAME_MAP.get(InstConfigVo::get_vo_type().as_str()).expect("TYPE_CONFIG_NAME_MAP is missing key"))
                    .await
                    .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client get_opt error: {e}"), ""))?
                    .ok_or_else(|| TardisError::wrap(&format!("[SG.admin] Kubernetes client error not found [{name}] config"), ""))?
                    .data
                    .ok_or_else(|| TardisError::wrap(&format!("[SG.admin] Kubernetes client found config [{name}] but data is None"), ""))?
                    .remove(name)
                    .ok_or_else(|| TardisError::wrap(&format!("[SG.admin] Kubernetes client found config [{name}] but cant find config:{name}"), ""))?
            } else {
                let cache_client = cache_client::get(DEFAULT_CLIENT_NAME).await?;
                cache_client
                    .hget(TYPE_CONFIG_NAME_MAP.get(InstConfigVo::get_vo_type().as_str()).expect(""), name)
                    .await?
                    .ok_or_else(|| TardisError::wrap(&format!("[SG.admin] Redis client not found [{name}] config"), ""))?
            };
            let config = TardisFuns::json.str_to_obj::<InstConfigVo>(&config_str)?;
            Ok(config.type_ == InstConfigType::K8sClusterConfig)
        }
    }
}
