use crate::model::query_dto::{BackendRefQueryDto, ToInstance};
use crate::model::vo::backend_vo::SgBackendRefVo;
use crate::service::backend_ref_service::BackendRefVoService;
use tardis::web::poem::web::Query;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Path;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct BackendApi;

/// Backend Ref API
#[poem_openapi::OpenApi(prefix_path = "/backend")]
impl BackendApi {
    /// Get Backend List
    #[oai(path = "/", method = "get")]
    async fn list(&self, names: Query<Option<String>>, namespace: Query<Option<String>>) -> TardisApiResult<Vec<SgBackendRefVo>> {
        let result = BackendRefVoService::list(
            BackendRefQueryDto {
                names: names.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                namespace: namespace.0,
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Backend
    #[oai(path = "/", method = "post")]
    async fn add(&self, backend: Json<SgBackendRefVo>) -> TardisApiResult<SgBackendRefVo> {
        TardisResp::ok(BackendRefVoService::add(backend.0).await?)
    }

    /// update Backend
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgBackendRefVo>) -> TardisApiResult<SgBackendRefVo> {
        TardisResp::ok(BackendRefVoService::update(backend.0).await?)
    }

    /// delete Backend
    #[oai(path = "/:backend_id", method = "put")]
    async fn delete(&self, backend_id: Path<String>) -> TardisApiResult<Void> {
        BackendRefVoService::delete(&backend_id.0).await?;
        TardisResp::ok(Void {})
    }
}
