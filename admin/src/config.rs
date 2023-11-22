use crate::config::k8s_config::K8sConfig;
use serde::Deserializer;
use tardis::serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacegateAdminConfig {
    /// If enable is true , then must use k8s configmap.
    /// Otherwise , use cache.
    pub is_kube: bool,

    #[serde(flatten)]
    pub kube_config: AdminK8sConfig,
}

#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AdminK8sConfig {
    pub k8s_config: Option<K8sConfig>,
    /// # `KUBECONFIG`
    ///
    /// ## Alternative operations
    ///
    /// ### Linux/macOS:
    /// ```bash
    /// export KUBECONFIG=/path/to/your/config
    ///```
    ///
    /// ### Windows:
    /// ```cmd
    /// set KUBECONFIG=<C>:\path\to\your\config
    /// ```
    pub kube_config: Option<String>,
}

pub mod k8s_config {
    use kube::config::NamedContext;
    use secrecy::SecretString;
    use serde::{Deserialize, Deserializer, Serialize};
    use tardis::web::poem_openapi;

    pub trait ToKubeconfig<T> {
        fn to_kubeconfig(self) -> T;
    }

    #[derive(Default, Debug, Serialize, Deserialize, poem_openapi::Object)]
    #[serde(default)]
    pub struct K8sConfig {
        /// Referencable names to cluster configs
        #[serde(default, deserialize_with = "deserialize_null_as_default")]
        pub clusters: NamedCluster,
        /// Referencable names to user configs
        #[serde(default, deserialize_with = "deserialize_null_as_default")]
        pub users: NamedAuthInfo,
    }
    impl ToKubeconfig<kube::config::Kubeconfig> for K8sConfig {
        fn to_kubeconfig(self) -> kube::config::Kubeconfig {
            let cluster = self.clusters.to_kubeconfig();
            let user = self.users.to_kubeconfig();
            let context = NamedContext {
                name: "default".to_string(),
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
                current_context: Some("default".to_string()),
                extensions: None,
                kind: Some("Config".to_string()),
                api_version: Some("v1".to_string()),
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Default, poem_openapi::Object)]
    pub struct NamedCluster {
        /// Name of cluster
        pub name: String,
        /// Information about how to communicate with a kubernetes cluster
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cluster: Option<Cluster>,
    }

    impl ToKubeconfig<kube::config::NamedCluster> for NamedCluster {
        fn to_kubeconfig(self) -> kube::config::NamedCluster {
            kube::config::NamedCluster {
                name: self.name,
                cluster: self.cluster.map(|c| c.to_kubeconfig()),
            }
        }
    }

    /// Cluster stores information to connect Kubernetes cluster.
    #[derive(Clone, Debug, Serialize, Deserialize, Default, poem_openapi::Object)]
    pub struct Cluster {
        /// The address of the kubernetes cluster (https://hostname:port).
        #[serde(skip_serializing_if = "Option::is_none")]
        pub server: Option<String>,
        /// Skips the validity check for the server's certificate. This will make your HTTPS connections insecure.
        #[serde(rename = "insecure-skip-tls-verify")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub insecure_skip_tls_verify: Option<bool>,
        /// The path to a cert file for the certificate authority.
        #[serde(rename = "certificate-authority")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub certificate_authority: Option<String>,
        /// PEM-encoded certificate authority certificates. Overrides `certificate_authority`
        #[serde(rename = "certificate-authority-data")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub certificate_authority_data: Option<String>,
        /// URL to the proxy to be used for all requests.
        #[serde(rename = "proxy-url")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub proxy_url: Option<String>,
        /// Name used to check server certificate.
        ///
        /// If `tls_server_name` is `None`, the hostname used to contact the server is used.
        #[serde(rename = "tls-server-name")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tls_server_name: Option<String>,
    }

    impl ToKubeconfig<kube::config::Cluster> for Cluster {
        fn to_kubeconfig(self) -> kube::config::Cluster {
            kube::config::Cluster {
                server: self.server,
                insecure_skip_tls_verify: self.insecure_skip_tls_verify,
                certificate_authority: self.certificate_authority,
                certificate_authority_data: self.certificate_authority_data,
                proxy_url: self.proxy_url,
                tls_server_name: self.tls_server_name,
                extensions: None,
            }
        }
    }

    /// NamedAuthInfo associates name with authentication.
    #[derive(Clone, Debug, Serialize, Deserialize, Default, poem_openapi::Object)]
    pub struct NamedAuthInfo {
        /// Name of the user
        pub name: String,
        /// Information that describes identity of the user
        #[serde(rename = "user")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub auth_info: Option<AuthInfo>,
    }

    impl ToKubeconfig<kube::config::NamedAuthInfo> for NamedAuthInfo {
        fn to_kubeconfig(self) -> kube::config::NamedAuthInfo {
            kube::config::NamedAuthInfo {
                name: self.name,
                auth_info: self.auth_info.map(|auth| auth.to_kubeconfig()),
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Default, poem_openapi::Object)]
    pub struct AuthInfo {
        /// The username for basic authentication to the kubernetes cluster.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub username: Option<String>,
        /// The password for basic authentication to the kubernetes cluster.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        pub password: Option<String>,

        /// The bearer token for authentication to the kubernetes cluster.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        pub token: Option<String>,
        /// Pointer to a file that contains a bearer token (as described above). If both `token` and token_file` are present, `token` takes precedence.
        #[serde(rename = "tokenFile")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub token_file: Option<String>,

        /// Path to a client cert file for TLS.
        #[serde(rename = "client-certificate")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub client_certificate: Option<String>,
        /// PEM-encoded data from a client cert file for TLS. Overrides `client_certificate`
        #[serde(rename = "client-certificate-data")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub client_certificate_data: Option<String>,

        /// Path to a client key file for TLS.
        #[serde(rename = "client-key")]
        #[serde(skip_serializing_if = "Option::is_none")]
        pub client_key: Option<String>,
        /// PEM-encoded data from a client key file for TLS. Overrides `client_key`
        #[serde(rename = "client-key-data")]
        #[serde(skip_serializing_if = "Option::is_none", default)]
        pub client_key_data: Option<String>,
    }

    impl ToKubeconfig<kube::config::AuthInfo> for AuthInfo {
        fn to_kubeconfig(self) -> kube::config::AuthInfo {
            kube::config::AuthInfo {
                username: self.username,
                password: self.password.map(|s| SecretString::new(s)),
                token: self.token.map(|token| SecretString::new(token)),
                token_file: self.token_file,
                client_certificate: self.client_certificate,
                client_certificate_data: self.client_certificate_data,
                client_key: self.client_key,
                client_key_data: self.client_key_data.map(|data| SecretString::new(data)),
                impersonate: None,
                impersonate_groups: None,
                auth_provider: None,
                exec: None,
            }
        }
    }

    fn deserialize_null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        T: Default + Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let opt = Option::deserialize(deserializer)?;
        Ok(opt.unwrap_or_default())
    }
}
