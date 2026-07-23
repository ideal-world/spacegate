use std::sync::Arc;

use k8s_gateway_api::{GatewayClass, GatewayClassStatus};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::Condition, NamespaceResourceScope};
use spacegate_model::{constants::GATEWAY_CONTROLLER_NAME, BoxResult};

pub mod convert;
pub mod create;
pub mod delete;
pub mod discovery;
// TODO check listen
pub mod listen;
pub mod retrieve;
pub mod update;

pub struct K8s {
    /// Namespace containing Gateway and Spacegate extension resources.
    pub namespace: Arc<str>,
    /// GatewayClass managed by this backend.
    pub gateway_class_name: Arc<str>,
    /// Optional Spacegate DaemonSet instance in `<name>[.<namespace>]` format.
    pub gateway_instance: Option<Arc<str>>,
    client: kube::Client,
}

/// Returns whether a Gateway belongs to the GatewayClass managed by this backend.
pub(crate) fn gateway_uses_class(gateway: &k8s_gateway_api::Gateway, gateway_class_name: &str) -> bool {
    gateway.spec.gateway_class_name == gateway_class_name
}

/// Replaces the GatewayClass Accepted condition while preserving unrelated conditions.
fn upsert_accepted_condition(status: &mut Option<GatewayClassStatus>, observed_generation: Option<i64>) {
    let accepted = Condition {
        last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(chrono::Utc::now()),
        message: "GatewayClass is managed by Spacegate".to_string(),
        reason: "Accepted".to_string(),
        status: "True".to_string(),
        type_: "Accepted".to_string(),
        observed_generation,
    };
    let status = status.get_or_insert_with(|| GatewayClassStatus { conditions: None });
    let conditions = status.conditions.get_or_insert_with(Vec::new);
    if let Some(condition) = conditions.iter_mut().find(|condition| condition.type_ == accepted.type_) {
        *condition = accepted;
    } else {
        conditions.insert(0, accepted);
    }
}

impl K8s {
    pub fn new(namespace: impl Into<Arc<str>>, client: kube::Client) -> Self {
        Self {
            namespace: namespace.into(),
            gateway_class_name: spacegate_model::constants::DEFAULT_GATEWAY_CLASS_NAME.into(),
            gateway_instance: None,
            client,
        }
    }

    pub async fn with_default_client(namespace: impl Into<Arc<str>>) -> Result<Self, kube::Error> {
        Ok(Self::new(namespace, kube::Client::try_default().await?))
    }

    /// Selects the GatewayClass and Spacegate instance managed by this backend.
    pub fn with_gateway_selection(mut self, gateway_class_name: impl Into<Arc<str>>, gateway_instance: Option<impl Into<Arc<str>>>) -> Self {
        self.gateway_class_name = gateway_class_name.into();
        self.gateway_instance = gateway_instance.map(Into::into);
        self
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

    pub fn get_specify_namespace_api<T: kube::Resource<Scope = NamespaceResourceScope>>(&self, ns: &str) -> kube::Api<T>
    where
        <T as kube::Resource>::DynamicType: Default,
    {
        kube::Api::namespaced(self.client.clone(), ns)
    }

    /// Marks a GatewayClass as accepted after verifying that Spacegate owns it.
    pub(crate) async fn accept_gateway_class(&self, name: &str) -> BoxResult<()> {
        let gateway_class_api: kube::Api<GatewayClass> = self.get_all_api();
        let mut gateway_class = gateway_class_api.get_status(name).await?;
        if gateway_class.spec.controller_name != GATEWAY_CONTROLLER_NAME {
            return Err(format!(
                "GatewayClass '{name}' is managed by '{}', expected '{GATEWAY_CONTROLLER_NAME}'",
                gateway_class.spec.controller_name
            )
            .into());
        }
        upsert_accepted_condition(&mut gateway_class.status, gateway_class.metadata.generation);
        gateway_class_api.replace_status(name, &kube::api::PostParams::default(), serde_json::to_vec(&gateway_class)?).await?;
        Ok(())
    }

    #[allow(unused, reason = "gateway permission")]
    pub(crate) async fn reject_gateway_class(&self, name: &str) -> BoxResult<()> {
        let condition = Condition {
            last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(chrono::Utc::now()),
            message: "Load config or refresh config , waiting for complete".to_string(),
            reason: "WaitingForController".to_string(),
            status: "False".to_string(),
            type_: "Progressing".to_string(),
            observed_generation: None,
        };
        let gateway_class_api: kube::Api<GatewayClass> = self.get_all_api();
        let mut gateway_class = gateway_class_api.get_status(name).await?;
        gateway_class.status = if let Some(mut status) = gateway_class.status {
            status.conditions = if let Some(mut conditions) = status.conditions {
                if let Some(condition) = conditions.first() {
                    if condition.status == "False" && condition.type_ == "Progressing" {
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

#[cfg(test)]
mod tests {
    use k8s_gateway_api::{Gateway, GatewayClassStatus, GatewaySpec};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Condition, Time};
    use kube::api::ObjectMeta;

    use super::{gateway_uses_class, upsert_accepted_condition};

    #[test]
    fn gateway_class_filter_uses_runtime_value() {
        let gateway = Gateway {
            metadata: ObjectMeta::default(),
            spec: GatewaySpec {
                gateway_class_name: "ai-spacegate".to_string(),
                listeners: Vec::new(),
                addresses: None,
            },
            status: None,
        };

        assert!(gateway_uses_class(&gateway, "ai-spacegate"));
        assert!(!gateway_uses_class(&gateway, "spacegate"));
    }

    #[test]
    fn accepted_condition_replaces_unknown_without_losing_other_conditions() {
        let mut status = Some(GatewayClassStatus {
            conditions: Some(vec![
                Condition {
                    last_transition_time: Time(chrono::Utc::now()),
                    message: "Waiting for controller".to_string(),
                    reason: "Pending".to_string(),
                    status: "Unknown".to_string(),
                    type_: "Accepted".to_string(),
                    observed_generation: None,
                },
                Condition {
                    last_transition_time: Time(chrono::Utc::now()),
                    message: "Parameters are valid".to_string(),
                    reason: "Resolved".to_string(),
                    status: "True".to_string(),
                    type_: "ResolvedRefs".to_string(),
                    observed_generation: Some(5),
                },
            ]),
        });

        upsert_accepted_condition(&mut status, Some(6));

        let conditions = status.and_then(|status| status.conditions).expect("conditions should exist");
        let accepted = conditions.iter().filter(|condition| condition.type_ == "Accepted").collect::<Vec<_>>();
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].status, "True");
        assert_eq!(accepted[0].reason, "Accepted");
        assert_eq!(accepted[0].observed_generation, Some(6));
        assert!(conditions.iter().any(|condition| condition.type_ == "ResolvedRefs"));
    }
}
