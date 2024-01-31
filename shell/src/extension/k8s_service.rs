use std::sync::Arc;

use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sServiceData {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct K8sService(pub Arc<K8sServiceData>);

impl ToString for K8sServiceData {
    fn to_string(&self) -> String {
        match self.namespace {
            Some(ref ns) => format!("{}.{}", self.name, ns),
            None => self.name.clone(),
        }
    }
}