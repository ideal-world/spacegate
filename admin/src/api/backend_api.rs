use crate::model::query_dto::{BackendRefQueryDto, ToInstance};
use crate::model::vo::backend_vo::SgBackendRefVo;
use crate::service::backend_ref_service::BackendRefVoService;
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::{Path, Query};
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct BackendApi;

/// Backend Ref API
#[poem_openapi::OpenApi(prefix_path = "/backend")]
impl BackendApi {
    /// Get Backend List
    #[oai(path = "/", method = "get")]
    async fn list(&self, names: Query<Option<String>>, namespace: Query<Option<String>>, hosts: Query<Option<String>>, session: &Session) -> TardisApiResult<Vec<SgBackendRefVo>> {
        let client_name = &super::get_instance_name(session).await?;
        let result = BackendRefVoService::list(
            client_name,
            BackendRefQueryDto {
                names: names.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                namespace: namespace.0,
                hosts: hosts.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Backend
    #[oai(path = "/", method = "post")]
    async fn add(&self, backend: Json<SgBackendRefVo>, session: &Session) -> TardisApiResult<SgBackendRefVo> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(BackendRefVoService::add(client_name, backend.0).await?)
    }

    /// update Backend
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgBackendRefVo>, session: &Session) -> TardisApiResult<SgBackendRefVo> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(BackendRefVoService::update(client_name, backend.0).await?)
    }

    /// delete Backend
    #[oai(path = "/:backend_id", method = "delete")]
    async fn delete(&self, backend_id: Path<String>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_instance_name(session).await?;
        BackendRefVoService::delete(client_name, &backend_id.0).await?;
        TardisResp::ok(Void {})
    }
}
