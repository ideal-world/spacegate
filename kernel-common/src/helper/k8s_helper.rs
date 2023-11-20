use crate::constants::k8s_constants::DEFAULT_NAMESPACE;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config, ResourceExt};
use lazy_static::lazy_static;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::tokio::sync::RwLock;

lazy_static! {
    static ref GLOBAL_CLIENT: RwLock<Option<Client>> = RwLock::default();
}

/// In k8s, names of resources need to be unique within a namespace
pub fn format_k8s_obj_unique(namespace: Option<&String>, name: &str) -> String {
    format!("{}.{}", namespace.unwrap_or(&DEFAULT_NAMESPACE.to_string()), name)
}

/// Get k8s object unique by object
pub fn get_k8s_obj_unique(obj: &impl kube::Resource) -> String {
    format_k8s_obj_unique(obj.namespace().as_ref(), obj.name_any().as_str())
}

/// Parse namespace and name from k8s unique name
/// return (namespace, name)
pub fn parse_k8s_obj_unique(unique_name: &str) -> (String, String) {
    let result = unique_name.split('.').collect::<Vec<&str>>();
    if result.len() != 2 {
        panic!("format_k8s_obj_unique failed");
    }
    (result[0].to_string(), result[1].to_string())
}

/// Try parse namespace and name from k8s unique name
/// return (namespace, name)
/// If parse failed,return (DEFAULT_NAMESPACE, name)
pub fn parse_k8s_unique_or_default(unique_name: &str) -> (String, String) {
    let result = unique_name.split('.').collect::<Vec<&str>>();
    if result.len() != 2 {
        (DEFAULT_NAMESPACE.to_string(), unique_name.to_string())
    } else {
        (result[0].to_string(), result[1].to_string())
    }
}

/// Warp `kube::Result` to `TardisResult`
pub trait WarpKubeResult<T> {
    fn warp_result(self) -> TardisResult<T>;
    fn warp_result_by_method(self, method: &str) -> TardisResult<T>;
}

impl<T> WarpKubeResult<T> for kube::Result<T> {
    fn warp_result(self) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.kube] Kubernetes api error:{e}"), ""))
    }

    fn warp_result_by_method(self, method: &str) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.kube] kubernetes api [{method}] error:{e}"), ""))
    }
}

pub async fn get_base_k8s_client() -> TardisResult<Client> {
    let global = GLOBAL_CLIENT.read().await;
    if let Some(client) = global.as_ref() {
        Ok(client.clone())
    } else {
        Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.admin] Get kubernetes client error: {error:?}"), ""))
    }
}

/// # Set Base kube client by file path
/// It only needs to be set once during initialization,
/// which conflicts with `set_base_k8s_client_by_config`
pub async fn set_base_k8s_client_by_file(path: &str) -> TardisResult<()> {
    let kube_config = Kubeconfig::read_from(path).map_err(|e| TardisError::conflict(&format!("[SG.admin] Read kubernetes config error:{e}"), ""))?;
    set_base_k8s_client_by_config(kube_config).await?;

    Ok(())
}

/// # Set Base kube client by `Kubeconfig`
/// It only needs to be set once during initialization,
/// which conflicts with `set_base_k8s_client_by_file`
pub async fn set_base_k8s_client_by_config(kube_config: Kubeconfig) -> TardisResult<()> {
    let client = get_k8s_client_by_config(kube_config).await?;
    let mut golabl = GLOBAL_CLIENT.write().await;
    *golabl = Some(client);

    Ok(())
}

/// # Get kube client by `Kubeconfig`
/// Instantiate `Kubeconfig` as client
pub async fn get_k8s_client_by_config(kube_config: Kubeconfig) -> TardisResult<Client> {
    let config = Config::from_custom_kubeconfig(kube_config, &KubeConfigOptions::default())
        .await
        .map_err(|e| TardisError::conflict(&format!("[SG.admin] Parse kubernetes config error:{e}"), ""))?;
    Ok(Client::try_from(config).map_err(|e| TardisError::conflict(&format!("[SG.admin] Create kubernetes client error:{e}"), ""))?)
}
