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
