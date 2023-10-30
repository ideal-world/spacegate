use std::collections::HashMap;

/// Get uid and version map
pub(crate) fn get_obj_uid_version_map(resources: &[impl kube::Resource]) -> HashMap<String, String> {
    resources.iter().map(|res| (res.meta().uid.clone().unwrap_or("".to_string()), res.meta().resource_version.clone().unwrap_or_default())).collect()
}
