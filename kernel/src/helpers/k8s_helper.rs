use std::collections::HashMap;

/// In k8s, names of resources need to be unique within a namespace
pub fn format_k8s_obj_unique(namespace: Option<&String>, name: &str) -> String {
    format!("{}.{}", namespace.unwrap_or(&"default".to_string()), name)
}

/// parse namespace and name from k8s unique name
pub fn parse_k8s_obj_unique(unique_name: &str) -> (String, String) {
    let result = unique_name.split('.').collect::<Vec<&str>>();
    if result.len() != 2 {
        panic!("format_k8s_obj_unique failed");
    }
    (result[0].to_string(), result[1].to_string())
}

/// get uid and version map
pub(crate) fn get_obj_uid_version_map(resources: &Vec<impl kube::Resource>) -> HashMap<String, String> {
    resources.iter().map(|res| (res.meta().uid.clone().unwrap_or("".to_string()), res.meta().resource_version.clone().unwrap_or_default())).collect()
}
