use kernel_common::helper::k8s_helper;
use kube::ResourceExt;
use std::collections::HashMap;

/// Get k8s object unique by object
pub(crate) fn get_k8s_obj_unique(obj: &impl kube::Resource) -> String {
    k8s_helper::format_k8s_obj_unique(obj.namespace().as_ref(), obj.name_any().as_str())
}

/// Get uid and version map
pub(crate) fn get_obj_uid_version_map(resources: &[impl kube::Resource]) -> HashMap<String, String> {
    resources.iter().map(|res| (res.meta().uid.clone().unwrap_or("".to_string()), res.meta().resource_version.clone().unwrap_or_default())).collect()
}
