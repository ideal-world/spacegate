use crate::model::query_dto::BackendRefQueryDto;
use crate::model::vo::backend_vo::BackendRefVO;
use crate::service::backend_ref_service::BackendRefService;
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
    async fn list(&self, name: Query<Option<String>>, namespace: Query<Option<String>>) -> TardisApiResult<Vec<BackendRefVO>> {
        let result = BackendRefService::list(
            namespace.0.clone(),
            BackendRefQueryDto {
                name: name.0,
                namespace: namespace.0,
            },
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Backend
    #[oai(path = "/", method = "post")]
    async fn add(&self, backend: Json<BackendRefVO>) -> TardisApiResult<Void> {
        BackendRefService::add(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// update Backend
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<BackendRefVO>) -> TardisApiResult<Void> {
        BackendRefService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// delete Backend
    #[oai(path = "/:backend_id", method = "put")]
    async fn delete(&self, backend_id: Path<String>) -> TardisApiResult<Void> {
        BackendRefService::delete(&backend_id.0).await?;
        TardisResp::ok(Void {})
    }
}
