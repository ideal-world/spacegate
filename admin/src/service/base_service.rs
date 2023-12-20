use crate::constants::{KUBE_VO_NAMESPACE, TYPE_CONFIG_NAME_MAP};
use crate::model::vo::Vo;
use k8s_openapi::{api::core::v1::ConfigMap, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use kernel_common::client::{cache_client, k8s_client};
use kube::{api::PostParams, Api};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use tardis::async_trait::async_trait;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::{log, TardisFuns};

use super::spacegate_manage_service::SpacegateManageService;

#[async_trait]
pub trait VoBaseService<T>
where
    T: Vo + Serialize + Sync + Send + DeserializeOwned,
{
    async fn get_str_type_map(client_name: &str) -> TardisResult<HashMap<String, String>> {
        let result = if SpacegateManageService::client_is_kube(client_name).await? {
            if let Some(t_config) = get_config_map_api(client_name)
                .await?
                .get_opt(&get_config_name::<T>())
                .await
                .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error: {e}"), ""))?
            {
                if let Some(b_map) = t_config.data {
                    b_map.into_iter().collect()
                } else {
                    HashMap::new()
                }
            } else {
                init_config_map_by_t::<T>(client_name).await?;
                HashMap::new()
            }
        } else {
            cache_client::get(client_name).await?.hgetall(&get_config_name::<T>()).await?
        };
        Ok(result)
    }

    async fn get_type_map(client_name: &str) -> TardisResult<HashMap<String, T>> {
        Ok(Self::get_str_type_map(client_name).await?.into_iter().map(|(k, v)| Ok((k, TardisFuns::json.str_to_obj::<T>(&v)?))).collect::<TardisResult<HashMap<String, T>>>()?)
    }

    async fn get_by_id_opt(client_name: &str, id: &str) -> TardisResult<Option<T>> {
        //todo optimze cache hget
        if let Some(t_str) = Self::get_str_type_map(client_name).await?.remove(id) {
            Ok(TardisFuns::json.str_to_obj(&t_str)?)
        } else {
            Ok(None)
        }
    }

    async fn get_by_id(client_name: &str, id: &str) -> TardisResult<T> {
        if let Some(t) = Self::get_by_id_opt(client_name, id).await? {
            Ok(t)
        } else {
            Err(TardisError::not_found(&format!("[SG.admin] Get Error: {}:{} not exists", T::get_vo_type(), id), ""))
        }
    }

    async fn add_vo(client_name: &str, config: T) -> TardisResult<T>
    where
        T: 'async_trait,
    {
        Self::add_or_update_vo(client_name, config, true, false).await
    }

    async fn update_vo(client_name: &str, config: T) -> TardisResult<T>
    where
        T: 'async_trait,
    {
        Self::add_or_update_vo(client_name, config, false, true).await
    }

    /// # add_or_update_vo
    /// **warning**: `add_only` and `update_only` cannot be true at the same time
    async fn add_or_update_vo(client_name: &str, config: T, add_only: bool, update_only: bool) -> TardisResult<T>
    where
        T: 'async_trait,
    {
        if add_only && update_only {
            panic!("add_only and update_only cannot be true at the same time");
        }

        let id = config.get_unique_name();
        let mut datas = Self::get_str_type_map(client_name).await?;
        if datas.get(&id).is_some() {
            if add_only {
                return Err(TardisError::bad_request(&format!("[SG.admin] {}:{} already exists", T::get_vo_type(), id), ""));
            } else {
                log::debug!("[SG.admin] add_or_update {}:{} exists , will update", T::get_vo_type(), id);
            }
        } else {
            if update_only {
                return Err(TardisError::bad_request(&format!("[SG.admin] {}:{} not exists", T::get_vo_type(), id), ""));
            } else {
                log::debug!("[SG.admin] add_or_update {}:{} not exists , will add", T::get_vo_type(), id);
            }
        }
        let config_str = serde_json::to_string(&config).map_err(|e| TardisError::bad_request(&format!("Serialization to json failed:{e}"), ""))?;
        if SpacegateManageService::client_is_kube(client_name).await? {
            datas.insert(id.clone(), config_str);
            get_config_map_api(client_name)
                .await?
                .replace(
                    &get_config_name::<T>(),
                    &PostParams::default(),
                    &ConfigMap {
                        data: Some(datas.into_iter().collect()),
                        metadata: ObjectMeta {
                            name: Some(get_config_name::<T>()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error:{e}"), ""))?;
        } else {
            cache_client::get(client_name).await?.hset(&get_config_name::<T>(), &id, &config_str).await?;
        }
        Ok(config)
    }

    async fn delete_vo(client_name: &str, config_id: &str) -> TardisResult<()> {
        if SpacegateManageService::client_is_kube(client_name).await? {
            let mut datas = Self::get_str_type_map(client_name).await?;
            if datas.remove(config_id).is_some() {
                get_config_map_api(client_name)
                    .await?
                    .replace(
                        &get_config_name::<T>(),
                        &PostParams::default(),
                        &ConfigMap {
                            data: Some(datas.into_iter().collect()),
                            metadata: ObjectMeta {
                                name: Some(get_config_name::<T>()),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error:{e}"), ""))?;
            } else {
                log::debug!("{}:{} already not exists", T::get_vo_type(), config_id);
            }
        } else {
            cache_client::get(client_name).await?.hdel(&get_config_name::<T>(), config_id).await?;
        }
        Ok(())
    }
}

pub async fn init_config_map_by_t<T>(client_name: &str) -> TardisResult<()>
where
    T: Vo,
{
    get_config_map_api(client_name)
        .await?
        .create(
            &PostParams::default(),
            &ConfigMap {
                immutable: Some(false),
                metadata: ObjectMeta {
                    name: Some(get_config_name::<T>()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error: {e}"), ""))?;
    Ok(())
}

#[inline]
pub fn get_config_name<T>() -> String
where
    T: Vo,
{
    TYPE_CONFIG_NAME_MAP.get(T::get_vo_type().as_str()).expect("").to_string()
}

#[inline]
pub async fn get_config_map_api(name: &str) -> TardisResult<Api<ConfigMap>> {
    Ok(Api::namespaced((*k8s_client::get(name).await?).clone(), KUBE_VO_NAMESPACE))
}

#[cfg(test)]
mod test {
    use crate::model::vo::backend_vo::SgBackendRefVo;
    use crate::model::vo::Vo;
    use crate::service::backend_ref_service::BackendRefVoService;
    use crate::service::base_service::VoBaseService;
    use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
    use tardis::tokio;

    #[tokio::test]
    #[ignore]
    async fn test() {
        let mut add_o_1 = SgBackendRefVo {
            id: "id34325".to_string(),
            name_or_host: "backend_name".to_string(),
            namespace: None,
            port: 678,
            timeout_ms: None,
            protocol: None,
            weight: None,
            filters: None,
            filter_vos: vec![],
        };
        BackendRefVoService::add_vo(DEFAULT_CLIENT_NAME, add_o_1.clone()).await.unwrap();
        assert!(BackendRefVoService::add_vo(DEFAULT_CLIENT_NAME, add_o_1.clone()).await.is_err());

        let get_o_1 =
            serde_json::from_str::<SgBackendRefVo>(BackendRefVoService::get_str_type_map(DEFAULT_CLIENT_NAME).await.unwrap().get(&add_o_1.get_unique_name()).unwrap()).unwrap();
        assert_eq!(get_o_1.port, add_o_1.port);

        add_o_1.port = 1832;
        BackendRefVoService::update_vo(DEFAULT_CLIENT_NAME, add_o_1.clone()).await.unwrap();

        let get_o_1 =
            serde_json::from_str::<SgBackendRefVo>(BackendRefVoService::get_str_type_map(DEFAULT_CLIENT_NAME).await.unwrap().get(&add_o_1.get_unique_name()).unwrap()).unwrap();
        assert_eq!(get_o_1.port, add_o_1.port);

        BackendRefVoService::delete_vo(DEFAULT_CLIENT_NAME, &add_o_1.get_unique_name()).await.unwrap();
    }
}
