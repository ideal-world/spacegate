use serde::{Deserialize, Serialize};

use k8s_openapi::schemars::JsonSchema;
use kube::CustomResource;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Document", group = "kube.rs", version = "v1", namespaced)]
pub struct DocumentSpec {
    name: String,
    author: String,
}
