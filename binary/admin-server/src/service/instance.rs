use std::sync::Arc;

pub enum Instance {}

pub struct K8sInstance {
    pub name: Arc<str>,
    pub namespace: Arc<str>,
}
