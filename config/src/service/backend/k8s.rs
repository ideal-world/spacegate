use std::sync::Arc;

pub struct K8s {
    pub namespace: Arc<str>,
}

impl K8s {
    pub fn new(namespace: impl Into<Arc<str>>) -> Self {
        Self {
            namespace: namespace.into()
        }
    }
}