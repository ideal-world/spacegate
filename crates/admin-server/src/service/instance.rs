use std::sync::Arc;

use super::config;

pub enum Instance {}

pub struct K8sInstance {
    pub name: Arc<str>,
    pub namespace: Arc<str>,
}


// impl K8sInstance {
//     pub async fn fetch_all() -> Vec<K8sInstance> {
//         config::k8s::fetch_all().await
//     }
// }