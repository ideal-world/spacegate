#[cfg(feature = "cache")]
pub mod cache_client {
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock};
    use tardis::basic::error::TardisError;
    use tardis::basic::result::TardisResult;
    use tardis::cache::cache_client::TardisCacheClient;
    use tardis::config::config_dto::CacheModuleConfig;
    use tardis::tokio::sync::RwLock;

    pub fn cache_clients() -> &'static RwLock<HashMap<String, Arc<TardisCacheClient>>> {
        static CACHE_CLIENTS: OnceLock<RwLock<HashMap<String, Arc<TardisCacheClient>>>> = OnceLock::new();
        CACHE_CLIENTS.get_or_init(Default::default)
    }

    pub async fn init(name: impl Into<String>, url: &str) -> TardisResult<()> {
        let cache = TardisCacheClient::init(&CacheModuleConfig::builder().url(url.parse().expect("invalid url")).build()).await?;
        {
            add(name, Arc::new(cache)).await?;
        }
        Ok(())
    }

    pub async fn add(name: impl Into<String>, cache: Arc<TardisCacheClient>) -> TardisResult<()> {
        let mut write = cache_clients().write().await;
        write.insert(name.into(), cache);
        Ok(())
    }

    pub async fn remove(name: &str) -> TardisResult<()> {
        {
            let mut write = cache_clients().write().await;
            write.remove(name);
        }
        Ok(())
    }

    pub async fn get(name: &str) -> TardisResult<Arc<TardisCacheClient>> {
        {
            let read = cache_clients().read().await;
            read.get(name).cloned().ok_or_else(|| TardisError::bad_request("[SG.common] Get cache client failed", ""))
        }
    }
}
#[cfg(feature = "k8s")]
pub mod k8s_client {
    use kube::config::{KubeConfigOptions, Kubeconfig};
    use kube::{Client, Config};
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock};
    use tardis::basic::error::TardisError;
    use tardis::basic::result::TardisResult;
    use tardis::tokio::sync::RwLock;

    pub static DEFAULT_CLIENT_NAME: &str = "default";

    pub fn k8s_clients() -> &'static RwLock<HashMap<String, Arc<kube::Client>>> {
        static K8S_CLIENTS: OnceLock<RwLock<HashMap<String, Arc<kube::Client>>>> = OnceLock::new();
        K8S_CLIENTS.get_or_init(Default::default)
    }

    pub async fn inst_by_path(name: impl Into<String>, path: &str) -> TardisResult<()> {
        let client = get_k8s_client_by_file(path).await?;
        {
            let mut write = k8s_clients().write().await;
            write.insert(name.into(), Arc::new(client));
        }
        Ok(())
    }

    pub async fn inst(name: impl Into<String>, config: Kubeconfig) -> TardisResult<()> {
        let client = get_k8s_client_by_config(config).await?;
        {
            let mut write = k8s_clients().write().await;
            write.insert(name.into(), Arc::new(client));
        }
        Ok(())
    }

    pub async fn remove(name: &str) -> TardisResult<()> {
        {
            let mut write = k8s_clients().write().await;
            write.remove(name);
        }
        Ok(())
    }

    pub async fn get(name: Option<&String>) -> TardisResult<Arc<kube::Client>> {
        {
            let read = k8s_clients().read().await;
            read.get(name.unwrap_or(&DEFAULT_CLIENT_NAME.to_string())).cloned().ok_or_else(|| TardisError::bad_request("[SG.common] Get k8s client failed", ""))
        }
    }

    /// # Get kube client by `Kubeconfig`
    /// Instantiate `Kubeconfig` as client
    pub async fn get_k8s_client_by_config(kube_config: Kubeconfig) -> TardisResult<Client> {
        let config = Config::from_custom_kubeconfig(kube_config, &KubeConfigOptions::default())
            .await
            .map_err(|e| TardisError::conflict(&format!("[SG.common] Parse kubernetes config error:{e}"), ""))?;
        Ok(Client::try_from(config).map_err(|e| TardisError::conflict(&format!("[SG.common] Create kubernetes client error:{e}"), ""))?)
    }

    /// # Get kube client by file path
    /// Instantiate file path as client
    pub async fn get_k8s_client_by_file(path: &str) -> TardisResult<Client> {
        let kube_config = Kubeconfig::read_from(path).map_err(|e| TardisError::conflict(&format!("[SG.common] Read kubernetes config error:{e}"), ""))?;

        Ok(get_k8s_client_by_config(kube_config).await?)
    }
}
