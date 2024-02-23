use std::sync::Arc;

use k8s_openapi::NamespaceResourceScope;

pub struct K8s {
    pub namespace: Arc<str>,
    client: kube::Client,
}

impl K8s {
    pub fn new(namespace: impl Into<Arc<str>>, client: kube::Client) -> Self {
        Self {
            namespace: namespace.into(),
            client,
        }
    }

    pub fn get_all_api<T: kube::Resource>(&self) -> kube::Api<T>
    where
        <T as kube::Resource>::DynamicType: Default,
    {
        kube::Api::all(self.client.clone())
    }

    pub fn get_namespace_api<T: kube::Resource<Scope = NamespaceResourceScope>>(&self) -> kube::Api<T>
    where
        <T as kube::Resource>::DynamicType: Default,
    {
        kube::Api::namespaced(self.client.clone(), &self.namespace)
    }
}
