use crate::config::k8s_config::ToKubeconfig;
use crate::config::SpacegateAdminConfig;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use kernel_common::client::{cache_client, k8s_client};
use kube::config::Kubeconfig;
use lazy_static::lazy_static;
use std::mem;
use std::sync::RwLock;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::TardisFunsInst;

lazy_static! {
    ///Total kube config
    pub static ref KUBECONFIG: RwLock<kube::config::Kubeconfig> = RwLock::default();
    pub static ref BASE_CLIENT_IS_KUBE: RwLock<bool> = RwLock::default();
}

pub async fn init_client(funs: &TardisFunsInst) -> TardisResult<()> {
    let config = funs.conf::<SpacegateAdminConfig>();
    let is_kube = BASE_CLIENT_IS_KUBE.write().await;
    *is_kube = config.is_kube;
    if config.is_kube {
        if let Some(path) = &config.kube_config.kube_config {
            k8s_client::inst_by_path(DEFAULT_CLIENT_NAME, path).await?;
        } else if let Some(k8s_config) = &config.kube_config.k8s_config {
            k8s_client::inst(DEFAULT_CLIENT_NAME, k8s_config.to_kubeconfig()).await?;
        } else {
            k8s_client::inst(DEFAULT_CLIENT_NAME, Kubeconfig::read().map_err(|e| TardisError::wrap(&format!(""), ""))?).await?;
        }
    } else {
        cache_client::add(DEFAULT_CLIENT_NAME, funs.cache()).await?;
    }
    Ok(())
}

pub async fn get_base_is_kube() -> TardisResult<bool> {
    let is_kube = BASE_CLIENT_IS_KUBE.read().await;
    Ok(is_kube)
}

// pub fn add_k8s_config(config: K8sClusterConfig) -> TardisResult<()> {
//     for _ in 0..100 {
//         if let Ok(mut kube_config) = KUBECONFIG.try_write() {
//             let mut swap_config = Kubeconfig::default();
//             mem::swap(&mut swap_config, &mut kube_config);
//             swap_config = swap_config.merge(config.to_kubeconfig()).map_err(|e| TardisError::wrap(&format!("kubeconfig parse error:{e}"), ""))?;
//
//             mem::swap(&mut swap_config, &mut kube_config);
//             return Ok(());
//         };
//     }
//
//     Err(TardisError::conflict("[SG.admin] add config: get lock time out", ""))
// }
