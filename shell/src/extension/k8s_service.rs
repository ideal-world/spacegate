use std::sync::Arc;

use spacegate_config::model::K8sServiceData;

/// Extension to store current k8s service
#[derive(Debug, Clone)]
pub struct K8sService(pub Arc<K8sServiceData>);
