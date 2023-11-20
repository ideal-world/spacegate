#[cfg(feature = "cache")]
mod cache_client {
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
            let mut write = cache_clients().write().await;
            write.insert(name.into(), Arc::new(cache));
        }
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
            read.get(name).cloned().ok_or_else(|| TardisError::bad_request("[SG.server] Get client failed", ""))
        }
    }
}
#[cfg(feature = "k8s")]
mod k8s_client {
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock};
    use tardis::basic::error::TardisError;
    use tardis::basic::result::TardisResult;
    use tardis::cache::cache_client::TardisCacheClient;
    // use tardis::cache::cache_client::TardisCacheClient;
    use tardis::config::config_dto::CacheModuleConfig;
    use tardis::tokio::sync::RwLock;

    pub fn k8s_clients() -> &'static RwLock<HashMap<String, Arc<kube::Client>>> {
        static K8S_CLIENTS: OnceLock<RwLock<HashMap<String, Arc<kube::Client>>>> = OnceLock::new();
        K8S_CLIENTS.get_or_init(Default::default)
    }

    pub async fn init(name: impl Into<String>, config: kube::Config) -> TardisResult<()> {
        kube::Client::try_from(config)
        // let cache = TardisCacheClient::init(&CacheModuleConfig::builder().url(url.parse().expect("invalid url")).build()).await?;
        // {
        //     let mut write = k8s_clients().write().await;
        //     write.insert(name.into(), Arc::new(cache));
        // }
        Ok(())
    }

    pub async fn remove(name: &str) -> TardisResult<()> {
        {
            let mut write = k8s_clients().write().await;
            write.remove(name);
        }
        Ok(())
    }

    pub async fn get(name: &str) -> TardisResult<Arc<TardisCacheClient>> {
        {
            let read = k8s_clients().read().await;
            read.get(name).cloned().ok_or_else(|| TardisError::bad_request("[SG.server] Get client failed", ""))
        }
    }
}
