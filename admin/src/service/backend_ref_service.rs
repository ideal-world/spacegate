use crate::model::query_dto::{BackendRefQueryInst, HttpRouteQueryDto, ToInstance as _};
use crate::model::vo::backend_vo::SgBackendRefVo;
use crate::model::vo::Vo as _;

use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::service::base_service::VoBaseService;

use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

use super::route_service::HttpRouteVoService;

pub struct BackendRefVoService;

impl VoBaseService<SgBackendRefVo> for BackendRefVoService {}

impl BackendRefVoService {
    pub(crate) async fn list(client_name: &str, query: BackendRefQueryInst) -> TardisResult<Vec<SgBackendRefVo>> {
        Ok(Self::get_type_map(client_name)
            .await?
            .into_values()
            .filter(|b|
                if let Some(q_names) = &query.names {
                    q_names.iter().any(|q| q.is_match(&b.id))
                } else {
                    true
                } &&
                    if let Some(namespace) = &query.namespace {
                        if let Some(b_namespace)=&b.namespace{
                            namespace.is_match(b_namespace)
                        }
                        else { false }
                } else {
                    true
                } && query.hosts.as_ref().map_or(true, |hosts| {
                    hosts.iter().any(|host| host.is_match(&b.name_or_host))
                })
            )
            .collect())
    }

    pub(crate) async fn add(client_name: &str, add: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        Self::add_vo(client_name, add).await
    }
    pub(crate) async fn update(client_name: &str, update: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        let id = update.id.clone();
        let result = Self::update_vo(client_name, update).await?;
        join_all(Self::get_ref_httproute(client_name, &id).await?.into_iter().map(|route| HttpRouteVoService::update(client_name, route)).collect::<Vec<_>>())
            .await
            .into_iter()
            .collect::<TardisResult<Vec<_>>>()?;
        Ok(result)
    }

    pub(crate) async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        let routes = Self::get_ref_httproute(client_name, id).await?;
        if routes.is_empty() {
            Self::delete_vo(client_name, id).await?;
            Ok(())
        } else {
            Err(TardisError::bad_request(
                &format!(
                    "[admin.service] Delete backend ref {id} is used by http route:{}",
                    routes.iter().map(|r| r.get_unique_name()).collect::<Vec<String>>().join(",")
                ),
                "",
            ))
        }
    }

    async fn get_ref_httproute(client_name: &str, id: &str) -> TardisResult<Vec<SgHttpRouteVo>> {
        HttpRouteVoService::list(
            client_name,
            HttpRouteQueryDto {
                names: None,
                gateway_name: None,
                hostnames: None,
                backend_ids: Some(vec![id.to_string()]),
                filter_ids: None,
            }
            .to_instance()?,
        )
        .await
    }
}
