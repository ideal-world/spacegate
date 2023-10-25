use crate::constants::TYPE_CONFIG_NAME_MAP;
use crate::helper::get_k8s_client;
use crate::model::vo::Vo;
use k8s_openapi::api::core::v1::ConfigMap;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::PostParams;
use kube::Api;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::log;

pub struct BaseService;

impl BaseService {
    #[cfg(feature = "k8s")]
    pub async fn get_type_map<'a, T>() -> TardisResult<HashMap<String, String>>
    where
        T: Vo + Deserialize<'a>,
    {
        if let Some(t_config) =
            get_config_map_api().await?.get_opt(&get_config_name::<T>()).await.map_err(|e| TardisError::io_error(&format!("[SG.admin] Kubernetes client error: {e}"), ""))?
        {
            if let Some(b_map) = t_config.data {
                Ok(b_map.into_iter().collect())
            } else {
                Ok(HashMap::new())
            }
        } else {
            init_config_map_by_t::<T>().await?;
            Ok(HashMap::new())
        }
    }


    #[cfg(feature = "k8s")]
    pub async fn get_by_id<'a,  T>(id: &str) -> TardisResult<Option<(String.,T)>>
        where
            T: Vo + Serialize + Deserialize<'a>,
    {
        Self::get_type_map::<T>().await?.get(id)
    }

    #[cfg(feature = "k8s")]
    pub async fn add<'a, T>(config: T) -> TardisResult<T>
    where
        T: Vo + Serialize + Deserialize<'a>,
    {
        Self::add_or_update::<T>(config, true).await
    }

    #[cfg(feature = "k8s")]
    pub async fn update<'a, T>(config: T) -> TardisResult<T>
    where
        T: Vo + Serialize + Deserialize<'a>,
    {
        Self::add_or_update::<T>(config, false).await
    }

    #[cfg(feature = "k8s")]
    pub async fn add_or_update<'a, T>(config: T, add_only: bool) -> TardisResult<T>
    where
        T: Vo + Serialize + Deserialize<'a>,
    {
        let id = config.get_unique_name();
        let mut datas = Self::get_type_map::<T>().await?;
        if let Some(_) = datas.get(&id) {
            if add_only {
                return Err(TardisError::bad_request(&format!("[SG.admin] {}:{} already exists", T::get_vo_type(), id), ""));
            } else {
                log::debug!("[SG.admin] add_or_update {}:{} exists , will update", T::get_vo_type(), id);
            }
        } else {
            log::debug!("[SG.admin] add_or_update {}:{} not exists , will add", T::get_vo_type(), id);
        }

        datas.insert(
            id.clone(),
            serde_json::to_string(&config).map_err(|e| TardisError::bad_request(&format!("Serialization to json failed:{e}"), ""))?,
        );
        get_config_map_api()
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
        Ok(config)
    }

    #[cfg(feature = "k8s")]
    pub async fn delete<'a, T>(config_id: &str) -> TardisResult<()>
    where
        T: Vo + Serialize + Deserialize<'a>,
    {
        let mut datas = Self::get_type_map::<T>().await?;
        if let Some(_) = datas.remove(config_id) {
            get_config_map_api()
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
        Ok(())
    }
}

#[cfg(feature = "k8s")]
pub async fn init_config_map_by_t<T>() -> TardisResult<()>
where
    T: Vo,
{
    get_config_map_api()
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

#[cfg(feature = "k8s")]
pub fn get_config_name<T>() -> String
where
    T: Vo,
{
    TYPE_CONFIG_NAME_MAP.get(T::get_vo_type().as_str()).expect("").to_string()
}

#[cfg(feature = "k8s")]
pub async fn get_config_map_api() -> TardisResult<Api<ConfigMap>> {
    Ok(Api::namespaced(get_k8s_client().await?, "spacegate"))
}

#[cfg(test)]
mod test {
    use crate::model::vo::backend_vo::BackendRefVO;
    use crate::model::vo::Vo;
    use crate::service::base_service::BaseService;
    use tardis::tokio;

    #[tokio::test]
    #[cfg(feature = "k8s")]
    #[ignore]
    async fn k8s_test() {
        let mut add_o_1 = BackendRefVO {
            id: "id34325".to_string(),
            name_or_host: "backend_name".to_string(),
            namespace: None,
            port: 678,
            timeout_ms: None,
            protocol: None,
            weight: None,
            filters: None,
        };
        BaseService::add(add_o_1.clone()).await.unwrap();
        assert!(BaseService::add(add_o_1.clone()).await.is_err());

        let get_o_1 = serde_json::from_str::<BackendRefVO>(&BaseService::get_type_map::<BackendRefVO>().await.unwrap().get(&add_o_1.get_unique_name()).unwrap()).unwrap();
        assert_eq!(get_o_1.port, add_o_1.port);

        add_o_1.port = 1832;
        BaseService::update(add_o_1.clone()).await.unwrap();

        let get_o_1 = serde_json::from_str::<BackendRefVO>(&BaseService::get_type_map::<BackendRefVO>().await.unwrap().get(&add_o_1.get_unique_name()).unwrap()).unwrap();
        assert_eq!(get_o_1.port, add_o_1.port);

        BaseService::delete::<BackendRefVO>(&add_o_1.get_unique_name()).await.unwrap();
    }
}