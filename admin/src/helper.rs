use k8s_gateway_api::Gateway;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;
#[cfg(feature = "k8s")]
use kube::Client;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

#[cfg(feature = "k8s")]
pub async fn get_k8s_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.admin] Get kubernetes client error: {error:?}"), ""))
}

#[cfg(feature = "k8s")]
pub trait WarpKubeResult<T> {
    fn warp_result(self) -> TardisResult<T>;
    fn warp_result_by_method(self, method: &str) -> TardisResult<T>;
}

#[cfg(feature = "k8s")]
impl<T> WarpKubeResult<T> for kube::Result<T> {
    fn warp_result(self) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.kube] kube api error:{e}"), ""))
    }

    fn warp_result_by_method(self, method: &str) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.kube] kube api [{method}] error:{e}"), ""))
    }
}
