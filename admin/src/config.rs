use tardis::serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacegateAdminConfig {
    #[cfg(feature = "k8s")]
    pub k8s_config: Option<K8sConfig>,
}

#[cfg(feature = "k8s")]
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct K8sConfig {
    /// The configured cluster url
    pub cluster_url: String,
    /// The configured default namespace
    pub default_namespace: String,
    /// The configured root certificate
    pub root_cert: Option<Vec<Vec<u8>>>,
    /// Set the timeout for connecting to the Kubernetes API.
    ///
    /// A value of `None` means no timeout
    pub connect_timeout: Option<std::time::Duration>,
    /// Set the timeout for the Kubernetes API response.
    ///
    /// A value of `None` means no timeout
    pub read_timeout: Option<std::time::Duration>,
    /// Set the timeout for the Kubernetes API request.
    ///
    /// A value of `None` means no timeout
    pub write_timeout: Option<std::time::Duration>,
    /// Whether to accept invalid certificates
    pub accept_invalid_certs: bool,
    /// Stores information to tell the cluster who you are.
    pub auth_info: kube::config::AuthInfo,
    // TODO Actually support proxy or create an example with custom client
    /// Optional proxy URL.
    pub proxy_url: Option<String>,
    /// If set, apiserver certificate will be validated to contain this string
    ///
    /// If not set, the `cluster_url` is used instead
    pub tls_server_name: Option<String>,
}
