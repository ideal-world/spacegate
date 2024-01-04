use crate::model::query_dto::{GatewayQueryDto, SgTlsQueryInst, ToInstance};
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::Vo;
use crate::service::base_service::VoBaseService;
use crate::service::gateway_service::GatewayVoService;
use k8s_openapi::api::core::v1::Secret;
use kernel_common::client::k8s_client;
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
use kernel_common::{
    helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult},
    inner_model::gateway::SgTls,
};
use kube::api::{DeleteParams, PostParams};
use kube::{Api, ResourceExt};
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

use super::spacegate_manage_service::SpacegateManageService;

pub struct TlsVoService;

impl VoBaseService<SgTls> for TlsVoService {}

impl TlsVoService {
    pub(crate) async fn list(client_name: &str, query: SgTlsQueryInst) -> TardisResult<Vec<SgTls>> {
        let map = Self::get_type_map(client_name).await?;
        if query.names.is_none() {
            Ok(map.into_values().collect())
        } else {
            Ok(map.into_values().filter(|t| query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&t.name)))).collect::<Vec<SgTls>>())
        }
    }

    pub(crate) async fn add(client_name: &str, mut add: SgTls) -> TardisResult<SgTls> {
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone();
        if is_kube {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let secret_api: Api<Secret> = Self::get_secret_api(client_name, &Some(namespace)).await?;
            let s = add_model.to_kube_tls();
            secret_api.create(&PostParams::default(), &s).await.warp_result_by_method("Add Secret")?;
        }
        Self::add_vo(client_name, add).await
    }

    pub(crate) async fn update(client_name: &str, update: SgTls) -> TardisResult<SgTls> {
        let unique_name = update.get_unique_name();
        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if let Some(_old_str) = Self::get_str_type_map(client_name).await?.remove(&unique_name) {
            if is_kube {
                let (namespace, name) = parse_k8s_obj_unique(&unique_name);
                let secret_api: Api<Secret> = Self::get_secret_api(client_name, &Some(namespace)).await?;
                let mut s = update.clone().to_kube_tls();
                s.metadata.resource_version =
                    secret_api.get_metadata(s.name_any().as_str()).await.warp_result_by_method("Get Metadata Before Update Secret")?.metadata.resource_version;
                secret_api.replace(&name, &PostParams::default(), &s).await.warp_result_by_method("Update Secret")?;
                Self::update_vo(client_name, update).await
            } else {
                let result = Self::update_vo(client_name, update).await?;
                join_all(
                    Self::get_ref_gateway(client_name, &unique_name).await?.into_iter().map(|ref_gateway| GatewayVoService::update(client_name, ref_gateway)).collect::<Vec<_>>(),
                )
                .await
                .into_iter()
                .collect::<TardisResult<Vec<_>>>()?;
                Ok(result)
            }
        } else {
            Err(TardisError::not_found(&format!("[admin.service] Update tls {} not found", unique_name), ""))
        }
    }

    pub(crate) async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        let ref_gateway = Self::get_ref_gateway(client_name, id).await?;
        if !ref_gateway.is_empty() {
            return Err(TardisError::bad_request(
                &format!(
                    "[admin.service] Delete tls {id} is used by gateway:{}",
                    ref_gateway.iter().map(|g| g.get_unique_name()).collect::<Vec<String>>().join(",")
                ),
                "",
            ));
        }

        let is_kube = SpacegateManageService::client_is_kube(client_name).await?;
        if is_kube {
            let (namespace, name) = parse_k8s_obj_unique(id);
            let secret_api: Api<Secret> = Self::get_secret_api(client_name, &Some(namespace)).await?;
            secret_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Secret")?;
        }
        let gateways = GatewayVoService::list(client_name, GatewayQueryDto { ..Default::default() }.to_instance()?).await?;
        if gateways.is_empty() {
            Self::delete_vo(client_name, id).await?;
            Ok(())
        } else {
            Err(TardisError::bad_request(
                &format!(
                    "[admin.service] Delete tls {id} is used by gateway:{}",
                    gateways.iter().map(|g| g.get_unique_name()).collect::<Vec<String>>().join(",")
                ),
                "",
            ))
        }
    }

    async fn get_ref_gateway(client_name: &str, id: &str) -> TardisResult<Vec<SgGatewayVo>> {
        GatewayVoService::list(
            client_name,
            GatewayQueryDto {
                tls_ids: Some(vec![id.to_string()]),
                ..Default::default()
            }
            .to_instance()?,
        )
        .await
    }

    async fn get_secret_api(client_name: &str, namespace: &Option<String>) -> TardisResult<Api<Secret>> {
        Ok(Api::namespaced(
            (*k8s_client::get(client_name).await?).clone(),
            namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()),
        ))
    }
}
