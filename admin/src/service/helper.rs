#[cfg(feature = "k8s")]
use kube::Client;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

#[cfg(feature = "k8s")]
pub async fn get_k8s_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.admin] Get kubernetes client error: {error:?}"), ""))
}

pub trait WarpKubeResult<T> {
    fn warp_result(self) -> TardisResult<T>;
    fn warp_result_by_method(self, method: &str) -> TardisResult<T>;
}

impl<T> WarpKubeResult<T> for kube::Result<T> {
    fn warp_result(self) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.admin] kube api error:{e}"), ""))
    }

    fn warp_result_by_method(self, method: &str) -> TardisResult<T> {
        self.map_err(|e| TardisError::wrap(&format!("[SG.admin] kube api [{method}] error:{e}"), ""))
    }
}
