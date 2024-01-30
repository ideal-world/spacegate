use std::sync::Arc;

pub struct K8s {
    pub namespace: Arc<str>,
    client: kube::Client,
}

impl K8s {
    pub fn new(namespace: impl Into<Arc<str>>, client: kube::Client) -> Self {
        Self {
            namespace: namespace.into(),
            client: client,
        }
    }

    pub fn get_all_api<T>(&self) -> kube::Api<T> {
        kube::Api::all(self.client.clone())
    }

    pub fn get_namespace_api<T>(&self) -> kube::Api<T> {
        kube::Api::namespaced(self.client.clone(), &self.namespace)
    }
}
