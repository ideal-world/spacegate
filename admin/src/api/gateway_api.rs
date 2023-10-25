use crate::model::query_dto::GatewayQueryDto;
use crate::model::vo::backend_vo::BackendRefVO;
use crate::service::gateway_service::GatewayService;
use crate::service::plugin_service::PluginService;
use kernel_common::inner_model::gateway::SgGateway;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};
use crate::model::vo::gateway_vo::SgGatewayVO;

#[derive(Clone, Default)]
pub struct GatewayApi;

/// Gateway API
#[poem_openapi::OpenApi(prefix_path = "/gateway")]
impl GatewayApi {
    /// Add Gateway
    #[oai(path = "/", method = "post")]
    async fn add(&self, backend: Json<BackendRefVO>) -> TardisApiResult<Void> {
        GatewayService::add(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Gateway
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgGatewayVO>) -> TardisApiResult<Void> {
        GatewayService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Gateway
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, namespace: Query<Option<String>>, name: Query<String>) -> TardisApiResult<Void> {
        GatewayService::delete(namespace.0, &name.0).await?;
        TardisResp::ok(Void {})
    }
}
