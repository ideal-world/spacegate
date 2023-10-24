use crate::helper::get_k8s_client;
use k8s_openapi::api::core::v1::ConfigMap;
use k8s_openapi::merge_strategies::list;
use kube::api::ListParams;
use kube::Api;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub const GATEWAY_CONFIG_NAME: &str = "gateway_config";
pub const PLUGIN_CONFIG_NAME: &str = "plugin_config";
pub const ROUTE_CONFIG_NAME: &str = "route_config";
pub const BACKEND_REF_CONFIG_NAME: &str = "backend_ref_config";

pub trait GetConfigMapName {
    fn get_config_map_name() -> String;
}

pub struct BaseService;

impl BaseService {
    #[cfg(feature = "k8s")]
    pub async fn list<'a, T>() -> TardisResult<HashMap<String, String>>
    where
        T: GetConfigMapName + Deserialize<'a>,
    {
        let mut items = get_config_map_api()
            .await?
            .list(&ListParams::default().fields(&format!("metadata.name={}", T::get_config_map_name())))
            .await
            .map_err(|e| TardisError::io_error(&format!("err:{e}"), ""))?
            .items;
        if items.is_empty() {
            Ok(HashMap::new())
        } else {
            if let Some(b_map) = items.remove(0).data {
                Ok(b_map.into_iter().collect())
            } else {
                Ok(HashMap::new())
            }
        }
    }

    #[cfg(feature = "k8s")]
    pub async fn add<'a, T>(config: T) -> TardisResult<()>
    where
        T: GetConfigMapName + Deserialize<'a>,
    {
        config
    }
}

#[cfg(feature = "k8s")]
pub async fn get_config_map_api() -> TardisResult<Api<ConfigMap>> {
    Ok(Api::namespaced(get_k8s_client().await?, "spacegate"))
}
