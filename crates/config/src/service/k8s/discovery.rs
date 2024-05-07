use k8s_openapi::api::core::v1::Service;
use kube::{api::ListParams, Api, ResourceExt};
use spacegate_model::{BackendHost, BoxResult, K8sServiceData};

use crate::service::Discovery;

use super::K8s;

impl Discovery for K8s {
    async fn api_url(&self) -> BoxResult<Option<String>> {
        todo!()
    }

    async fn backends(&self) -> BoxResult<Vec<BackendHost>> {
        let service_api: Api<Service> = self.get_all_api();
        let result = service_api
            .list(&ListParams::default())
            .await?
            .into_iter()
            .map(|s| {
                BackendHost::K8sService(K8sServiceData {
                    name: s.name_any(),
                    namespace: s.namespace(),
                })
            })
            .collect();
        Ok(result)
    }
}
