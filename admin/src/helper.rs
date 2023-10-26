#[cfg(feature = "k8s")]
use kube::Client;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

#[cfg(feature = "k8s")]
pub async fn get_k8s_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.admin] Get kubernetes client error: {error:?}"), ""))
}
