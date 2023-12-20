use crate::config::k8s_config::ToKubeconfig;
use crate::config::SpacegateAdminConfig;
use crate::model::query_dto::SpacegateInstQueryInst;
use crate::model::vo::spacegate_inst_vo::InstConfigType;
use crate::model::vo::Vo;
use crate::service::spacegate_manage_service::SpacegateManageService;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use kernel_common::client::{cache_client, k8s_client};
use kube::config::Kubeconfig;
use lazy_static::lazy_static;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::tokio::sync::RwLock;
use tardis::{log, TardisFunsInst};

lazy_static! {
    pub static ref BASE_CLIENT_IS_KUBE: RwLock<bool> = RwLock::default();
}

pub async fn init_client(funs: &TardisFunsInst) -> TardisResult<()> {
    {
        let config = funs.conf::<SpacegateAdminConfig>();
        let mut is_kube = BASE_CLIENT_IS_KUBE.write().await;
        *is_kube = config.is_kube;
        if config.is_kube {
            if let Some(path) = &config.kube_config.kube_config {
                k8s_client::inst_by_path(DEFAULT_CLIENT_NAME, path).await?;
            } else if let Some(k8s_config) = &config.kube_config.k8s_config {
                k8s_client::inst(DEFAULT_CLIENT_NAME, k8s_config.clone().to_kubeconfig()).await?;
            } else {
                k8s_client::inst(
                    DEFAULT_CLIENT_NAME,
                    Kubeconfig::read().map_err(|e| TardisError::wrap(&format!("init k8s client failed:{e}"), ""))?,
                )
                .await?;
            }
        } else {
            cache_client::add(DEFAULT_CLIENT_NAME, funs.cache()).await?;
        }
        log::info!("[Admin.init_client] Init base client[{}] success", if config.is_kube { "k8s" } else { "cache" });
    }
    init_client_by_default_client().await?;
    Ok(())
}

pub async fn init_client_by_default_client() -> TardisResult<()> {
    for inst_vo in SpacegateManageService::list(SpacegateInstQueryInst { names: None }).await? {
        log::info!("[admin.init_client] Init client {}, type: {:?}", inst_vo.get_unique_name(), inst_vo.type_);
        match inst_vo.type_ {
            InstConfigType::K8sClusterConfig => {
                if inst_vo.k8s_cluster_config.is_none() {
                    return Err(TardisError::bad_request("[admin.init_client] k8s_cluster_config is required", ""));
                }
            }
            InstConfigType::RedisConfig => {
                if inst_vo.redis_config.is_none() {
                    return Err(TardisError::bad_request("[admin.init_client] redis_config is required", ""));
                }
            }
        }
        let name = inst_vo.get_unique_name();
        if name == DEFAULT_CLIENT_NAME || name.is_empty() {
            return Err(TardisError::bad_request(
                &format!("[admin.init_client] client name {DEFAULT_CLIENT_NAME} is not allowed"),
                "",
            ));
        }
        match inst_vo.type_ {
            InstConfigType::K8sClusterConfig => {
                let config = inst_vo.k8s_cluster_config.clone().expect("").to_kubeconfig();
                k8s_client::inst(name, config).await?;
            }
            InstConfigType::RedisConfig => {
                cache_client::init(name, &inst_vo.redis_config.clone().expect("").url).await?;
            }
        }
    }
    Ok(())
}

pub async fn get_base_is_kube() -> TardisResult<bool> {
    let is_kube = BASE_CLIENT_IS_KUBE.read().await;
    Ok(*is_kube)
}
