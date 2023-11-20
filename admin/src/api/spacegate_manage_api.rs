use crate::model::vo::spacegate_inst_vo::InstConfigVo;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct SpacegateManageApi;

#[poem_openapi::OpenApi(prefix_path = "/spacegate/manage")]
impl SpacegateManageApi {
    /// List Spacegate Inst
    #[oai(path = "/", method = "get")]
    async fn list(&self) -> TardisApiResult<Vec<InstConfigVo>> {
        SpacegateManageService::list().await?;
    }

    /// Add Spacegate Inst
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<InstConfigVo>) -> TardisApiResult<Void> {
        // HttpRouteVoService::add(add.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Spacegate Inst
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<InstConfigVo>) -> TardisApiResult<Void> {
        // HttpRouteVoService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Spacegate Inst
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        // HttpRouteVoService::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
