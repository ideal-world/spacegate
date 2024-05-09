use k8s_openapi::api::{
    apps::v1::DaemonSet,
    core::v1::{Pod, Service},
};
use kube::{api::ListParams, Api, ResourceExt};
use rand::Rng as _;
use spacegate_model::{BackendHost, BoxResult, K8sServiceData};

use crate::service::Discovery;

use super::K8s;

impl Discovery for K8s {
    async fn api_url(&self) -> BoxResult<Option<String>> {
        // TODO Start from GatewayClass and look down, and read api port from GatewayClass
        let pod_api: Api<Pod> = self.get_namespace_api();
        let ds_api: Api<DaemonSet> = self.get_namespace_api();
        let dss = ds_api.list(&ListParams::default()).await?;
        let pods = pod_api.list(&ListParams::default()).await?;

        let mut pods = pods.items;
        pods.retain(|p| {
            for owner_ref in p.owner_references() {
                for ds in &dss {
                    if owner_ref.uid == ds.uid().unwrap_or_default() && owner_ref.name == ds.name_any() {
                        return true;
                    }
                }
            }
            false
        });

        if pods.is_empty() {
            return Ok(None);
        }
        let index = rand::thread_rng().gen_range(0..pods.len());
        let rand_pod = pods.get(index).expect("pods should not be empty");
        if let Some(host_ip) = rand_pod.status.clone().and_then(|s| s.host_ip) {
            return Ok(Some(format!("{host_ip}:{}", spacegate_model::constants::DEFAULT_API_PORT)));
        };
        Ok(None)
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
