#[cfg(feature = "cache")]
pub mod cache_client {
    use k8s_openapi::chrono::Utc;
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock};
    use tardis::basic::error::TardisError;
    use tardis::basic::result::TardisResult;
    use tardis::cache::cache_client::TardisCacheClient;
    use tardis::config::config_dto::CacheModuleConfig;
    use tardis::log;
    use tardis::tokio::sync::RwLock;

    use crate::inner_model::http_route::SgHttpRoute;

    //todo merge with kernel/src/config/config_by_redis.rs
    /// hash: {gateway name} -> {gateway config}
    pub const CONF_GATEWAY_KEY: &str = "sg:conf:gateway";
    /// list: {gateway name} -> {vec<http route config>}
    pub const CONF_HTTP_ROUTE_KEY: &str = "sg:conf:route:http:";
    /// string: {timestamp}##{changed obj}##{changed gateway name} -> None
    pub const CONF_CHANGE_TRIGGER: &str = "sg:conf:change:trigger:";

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
        let mut write = cache_clients().write().await;
        write.remove(name);

        Ok(())
    }

    pub async fn get(name: &str) -> TardisResult<Arc<TardisCacheClient>> {
        let read = cache_clients().read().await;
        read.get(name).cloned().ok_or_else(|| TardisError::bad_request(&format!("[SG.common] Get cache client [{name}] failed"), ""))
    }

    /// # Add orUpdate object
    /// parameters:
    /// - `client_name`: cache client name
    /// - `gateway_name`: if `obj_type` is `CONF_GATEWAY_KEY`, `gateway_name` is equal to `obj_name`,else `gateway_name` is httproute rel name
    /// - `obj_type`: object type can be `CONF_GATEWAY_KEY` or `CONF_HTTP_ROUTE_KEY`
    /// - `obj_name`: object name
    /// - `obj`: object value
    pub async fn add_or_update_obj(client_name: &str, obj_type: &str, gateway_name: &str, obj_name: &str, obj: &str) -> TardisResult<()> {
        let client = get(client_name).await?;
        match obj_type {
            CONF_GATEWAY_KEY => client.hset(CONF_GATEWAY_KEY, obj_name, obj).await?,
            CONF_HTTP_ROUTE_KEY => {
                let key = format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_name);
                let old_httproutes = client
                    .lrangeall(&key)
                    .await?
                    .into_iter()
                    .map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).expect("[SG.config] Route config parse error"))
                    .collect::<Vec<SgHttpRoute>>();
                if let Some(index) = old_httproutes.iter().position(|x| x.name == obj_name) {
                    client.lset(&key, index as isize, obj).await?;
                } else {
                    client.lpush(&key, obj).await?
                }
            }
            _ => return Err(TardisError::bad_request("[SG.common] Add or update object failed: invalid obj type", "")),
        }
        set_trigger(client_name, obj_type, gateway_name).await?;
        Ok(())
    }

    pub async fn delete_obj(client_name: &str, obj_type: &str, gateway_name: &str, obj_name: &str) -> TardisResult<()> {
        let client = get(client_name).await?;
        match obj_type {
            CONF_GATEWAY_KEY => client.hdel(CONF_GATEWAY_KEY, obj_name).await?,
            CONF_HTTP_ROUTE_KEY => {
                let key = format!("{CONF_HTTP_ROUTE_KEY}{}", gateway_name);
                let old_httproutes = client
                    .lrangeall(&key)
                    .await?
                    .into_iter()
                    .map(|v| tardis::TardisFuns::json.str_to_obj::<SgHttpRoute>(&v).expect("[SG.config] Route config parse error"))
                    .collect::<Vec<SgHttpRoute>>();
                if let Some(delete_httproute) = old_httproutes.into_iter().find(|x| x.name == obj_name) {
                    client.lrem(&key, 1, &tardis::TardisFuns::json.obj_to_string(&delete_httproute)?).await?;
                } else {
                    log::info!("[SG.common] delete obj not found:{}", obj_name);
                }
            }
            _ => return Err(TardisError::bad_request("[SG.common] Add or update object failed: invalid obj type", "")),
        }
        set_trigger(client_name, obj_type, gateway_name).await?;
        Ok(())
    }

    pub async fn set_trigger(client_name: &str, change_type: &str, change_gateway_name: &str) -> TardisResult<()> {
        let client = get(client_name).await?;
        client
            .set(&format!("{CONF_CHANGE_TRIGGER}{}##{}##{}", Utc::now().timestamp(), change_type, change_gateway_name), "")
            .await
            .map_err(|e| TardisError::wrap(&format!("[SG.common] Set trigger failed:{e}"), ""))
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

    pub async fn get(mut name: &str) -> TardisResult<Arc<kube::Client>> {
        if name.is_empty() {
            name = DEFAULT_CLIENT_NAME;
        }
        let read = k8s_clients().read().await;
        read.get(name).cloned().ok_or_else(|| TardisError::bad_request("[SG.common] Get k8s client failed", ""))
    }

    /// # Get kube client by `Kubeconfig`
    /// Instantiate `Kubeconfig` as client
    pub async fn get_k8s_client_by_config(kube_config: Kubeconfig) -> TardisResult<Client> {
        let config = Config::from_custom_kubeconfig(kube_config, &KubeConfigOptions::default())
            .await
            .map_err(|e| TardisError::conflict(&format!("[SG.common] Parse kubernetes config error:{e}"), ""))?;
        Client::try_from(config).map_err(|e| TardisError::conflict(&format!("[SG.common] Create kubernetes client error:{e}"), ""))
    }

    /// # Get kube client by file path
    /// Instantiate file path as client
    pub async fn get_k8s_client_by_file(path: &str) -> TardisResult<Client> {
        let kube_config = Kubeconfig::read_from(path).map_err(|e| TardisError::conflict(&format!("[SG.common] Read kubernetes config error:{e}"), ""))?;

        get_k8s_client_by_config(kube_config).await
    }
}
