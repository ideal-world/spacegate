use crate::config::k8s_config::{K8sConfig, ToKubeconfig};
use crate::constants;
use crate::model::vo::Vo;
use kube::config::NamedContext;
use serde::{Deserialize, Serialize};
use tardis::web::poem_openapi;

#[derive(Debug, Serialize, Deserialize, poem_openapi::Object)]
pub struct InstConfigVo {
    pub type_: InstConfigType,
    pub k8s_cluster_config: Option<K8sClusterConfig>,
    pub redis_config: Option<RedisConfig>,
}

impl Vo for InstConfigVo {
    fn get_vo_type() -> String {
        constants::INSTANCE_TYPE.to_string()
    }

    fn get_unique_name(&self) -> String {
        match &self.type_ {
            InstConfigType::K8sClusterConfig => {
                self.k8s_cluster_config.as_ref().unwrap_or_else(|| panic!("[admin] have inst config {self:?} type is k8s cluster config , but not found ")).name.clone()
            }
            InstConfigType::RedisConfig => {
                self.redis_config.as_ref().unwrap_or_else(|| panic!("[admin] have inst config {self:?} type is redis config , but not found ")).name.clone()
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, poem_openapi::Enum)]
pub enum InstConfigType {
    K8sClusterConfig,
    RedisConfig,
}

#[derive(Debug, Default, Serialize, Clone, Deserialize, poem_openapi::Object)]
#[serde(default)]
pub struct K8sClusterConfig {
    /// uid
    pub name: String,
    #[serde(flatten)]
    pub config: K8sConfig,
}

impl ToKubeconfig<kube::config::Kubeconfig> for K8sClusterConfig {
    fn to_kubeconfig(self) -> kube::config::Kubeconfig {
        let cluster = self.config.clusters.to_kubeconfig();
        let user = self.config.users.to_kubeconfig();
        let context = NamedContext {
            name: self.name.clone(),
            context: Some(kube::config::Context {
                cluster: cluster.name.clone(),
                user: user.name.clone(),
                namespace: None,
                extensions: None,
            }),
        };
        kube::config::Kubeconfig {
            preferences: None,
            clusters: vec![cluster],
            auth_infos: vec![user],
            contexts: vec![context],
            current_context: Some(self.name),
            extensions: None,
            kind: Some("Config".to_string()),
            api_version: Some("v1".to_string()),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, poem_openapi::Object)]
pub struct RedisConfig {
    /// uid
    pub name: String,
    pub url: String,
}
