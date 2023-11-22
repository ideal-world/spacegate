use crate::constants::k8s_constants::DEFAULT_NAMESPACE;
use kube::ResourceExt;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

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
