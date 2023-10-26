use crate::model::vo::gateway_vo::SgGatewayVO;
use crate::service::gateway_service::GatewayServiceVo;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct GatewayApi;

/// Gateway API
#[poem_openapi::OpenApi(prefix_path = "/gateway")]
impl GatewayApi {
    /// Add Gateway
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgGatewayVO>) -> TardisApiResult<Void> {
        GatewayServiceVo::add(add.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Gateway
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgGatewayVO>) -> TardisApiResult<Void> {
        GatewayServiceVo::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Gateway
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        GatewayServiceVo::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
