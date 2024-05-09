use std::sync::Arc;

use k8s_gateway_api::{GatewayClass, GatewayClassStatus};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::Condition, NamespaceResourceScope};
use spacegate_model::BoxResult;

pub mod convert;
pub mod create;
pub mod delete;
pub mod discovery;
// TODO check listen
pub mod listen;
pub mod retrieve;
pub mod update;

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

    pub async fn with_default_client(namespace: impl Into<Arc<str>>) -> Result<Self, kube::Error> {
        Ok(Self {
            namespace: namespace.into(),
            client: kube::Client::try_default().await?,
        })
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

    pub(crate) async fn accept_gateway_class(&self, name: &str) -> BoxResult<()> {
        let condition = Condition {
            last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(chrono::Utc::now()),
            message: "Accepted".to_string(),
            reason: "".to_string(),
            status: "True".to_string(),
            type_: "Accepted".to_string(),
            observed_generation: None,
        };
        let gateway_class_api: kube::Api<GatewayClass> = self.get_all_api();
        let mut gateway_class = gateway_class_api.get_status(name).await?;
        gateway_class.status = if let Some(mut status) = gateway_class.status {
            status.conditions = if let Some(mut conditions) = status.conditions {
                for condition in &conditions {
                    if condition.status == "True" && condition.type_ == "Accepted" {
                        return Ok(());
                    }
                }
                conditions.push(condition);
                Some(conditions)
            } else {
                Some(vec![condition])
            };

            Some(status)
        } else {
            Some(GatewayClassStatus {
                conditions: Some(vec![condition]),
            })
        };
        gateway_class_api.replace_status(name, &kube::api::PostParams::default(), serde_json::to_vec(&gateway_class)?).await?;
        Ok(())
    }

    pub(crate) async fn reject_gateway_class(&self, name: &str) -> BoxResult<()> {
        let condition = Condition {
            last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(chrono::Utc::now()),
            message: "Load config or refresh config,waiting for complete".to_string(),
            reason: "".to_string(),
            status: "False".to_string(),
            type_: "Accepted".to_string(),
            observed_generation: None,
        };
        let gateway_class_api: kube::Api<GatewayClass> = self.get_all_api();
        let mut gateway_class = gateway_class_api.get_status(name).await?;
        gateway_class.status = if let Some(mut status) = gateway_class.status {
            status.conditions = if let Some(mut conditions) = status.conditions {
                for condition in conditions {
                    if condition.status == "False" && condition.type_ == "Accepted" {
                        return Ok(());
                    }
                }
                conditions = vec![condition];
                Some(conditions)
            } else {
                Some(vec![condition])
            };

            Some(status)
        } else {
            Some(GatewayClassStatus {
                conditions: Some(vec![condition]),
            })
        };
        gateway_class_api.replace_status(name, &kube::api::PostParams::default(), serde_json::to_vec(&gateway_class)?).await?;
        Ok(())
    }
}
