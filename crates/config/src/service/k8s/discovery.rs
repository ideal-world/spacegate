use k8s_gateway_api::GatewayClass;
use k8s_openapi::api::{
    apps::v1::DaemonSet,
    core::v1::{Pod, Service},
};
use kube::{api::ListParams, Api, ResourceExt};
use spacegate_model::{constants::DEFAULT_API_PORT, BackendHost, BoxResult, K8sServiceData};

use crate::service::{Discovery, Instance};

use super::K8s;

pub struct K8sGatewayInstance {
    name: String,
    uri: String,
}
impl Instance for K8sGatewayInstance {
    fn api_url(&self) -> &str {
        &self.uri
    }
    fn id(&self) -> &str {
        &self.name
    }
}
impl Discovery for K8s {
    async fn instances(&self) -> BoxResult<Vec<impl Instance>> {
        let gateway_class_api: Api<GatewayClass> = self.get_all_api();

        let instance = if let Some(gateway_class) = gateway_class_api.get_opt(&self.gateway_class_name).await? {
            resolve_gateway_instance(self.gateway_instance.as_deref(), gateway_class.labels())
        } else {
            return Err(format!("gateway class '{}' not found", self.gateway_class_name).into());
        };

        let instance_split: Vec<_> = instance.split('.').collect();
        let (ds_api, pod_api, ds_name): (Api<DaemonSet>, Api<Pod>, String) = if instance_split.len() == 2 {
            let ns = instance_split.get(1).expect("unexpected");
            let ds_api: Api<DaemonSet> = self.get_specify_namespace_api(ns);
            let pod_api: Api<Pod> = self.get_specify_namespace_api(ns);
            let instance_name = instance_split.first().expect("unexpected");
            (ds_api, pod_api, instance_name.to_string())
        } else {
            let ds_api: Api<DaemonSet> = self.get_namespace_api();
            let pod_api: Api<Pod> = self.get_namespace_api();
            let instance_name = instance;
            (ds_api, pod_api, instance_name)
        };

        let ds_instance = if let Some(ds) = ds_api.get_opt(&ds_name).await? {
            ds
        } else {
            return Err("spacegate instance not found".into());
        };

        let pods = pod_api.list(&ListParams::default()).await?;
        let pods = pods.items;
        let instance_list = pods
            .into_iter()
            .filter_map(|p| {
                let ip = p.status.as_ref().and_then(|s| s.host_ip.as_ref())?;
                let port = DEFAULT_API_PORT;
                for owner_ref in p.owner_references() {
                    let instance_name = ds_instance.name_any();

                    if owner_ref.uid == ds_instance.uid().unwrap_or_default() && owner_ref.name == instance_name {
                        return Some(K8sGatewayInstance {
                            name: instance_name,
                            uri: format!("{ip}:{port}"),
                        });
                    }
                }
                None
            })
            .collect();

        Ok(instance_list)
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

/// Resolves the Spacegate DaemonSet from explicit configuration, GatewayClass labels, or the legacy default.
fn resolve_gateway_instance(explicit_instance: Option<&str>, labels: &std::collections::BTreeMap<String, String>) -> String {
    explicit_instance
        .map(str::trim)
        .filter(|instance| !instance.is_empty())
        .or_else(|| labels.get(spacegate_model::constants::KUBE_OBJECT_INSTANCE).map(String::as_str))
        .unwrap_or(spacegate_model::constants::DEFAULT_GATEWAY_INSTANCE)
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::resolve_gateway_instance;

    #[test]
    fn explicit_gateway_instance_overrides_gateway_class_label() {
        let labels = BTreeMap::from([(spacegate_model::constants::KUBE_OBJECT_INSTANCE.to_string(), "label-spacegate.label-namespace".to_string())]);

        assert_eq!(resolve_gateway_instance(Some("ai-spacegate.ai-hai"), &labels), "ai-spacegate.ai-hai");
    }

    #[test]
    fn gateway_instance_falls_back_to_label_then_legacy_default() {
        let labels = BTreeMap::from([(spacegate_model::constants::KUBE_OBJECT_INSTANCE.to_string(), "label-spacegate.label-namespace".to_string())]);

        assert_eq!(resolve_gateway_instance(None, &labels), "label-spacegate.label-namespace");
        assert_eq!(resolve_gateway_instance(None, &BTreeMap::new()), spacegate_model::constants::DEFAULT_GATEWAY_INSTANCE);
    }
}
