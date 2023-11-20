use crate::config::k8s_config::ToKubeconfig;
use crate::model::vo::spacegate_inst_vo::K8sClusterConfig;
use kube::config::Kubeconfig;
use lazy_static::lazy_static;
use std::mem;
use std::sync::RwLock;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

lazy_static! {
    ///Total kube config
    pub static ref KUBECONFIG: RwLock<kube::config::Kubeconfig> = RwLock::default();
}

pub fn add_k8s_config(config: K8sClusterConfig) -> TardisResult<()> {
    for _ in 0..100 {
        if let Ok(mut kube_config) = KUBECONFIG.try_write() {
            let mut swap_config = Kubeconfig::default();
            mem::swap(&mut swap_config, &mut kube_config);
            swap_config = swap_config.merge(config.to_kubeconfig()).map_err(|e| TardisError::wrap(&format!("kubeconfig parse error:{e}"), ""))?;

            mem::swap(&mut swap_config, &mut kube_config);
            return Ok(());
        };
    }

    Err(TardisError::conflict("[SG.admin] add config: get lock time out", ""))
}
