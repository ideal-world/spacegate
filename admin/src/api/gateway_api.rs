use crate::dto::base_dto::CommonPageDto;
use crate::dto::query_dto::GatewayQueryDto;
use crate::service::gateway_service::GatewayService;
use kernel_dto::dto::gateway_dto::SgGateway;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisPage, TardisResp, Void};

#[derive(Clone, Default)]
pub struct GatewayApi;

/// Gateway API
#[poem_openapi::OpenApi(prefix_path = "/gateway")]
impl GatewayApi {
    /// Get Gateway List
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        name: Query<Option<String>>,
        namespace: Query<Option<String>>,
        port: Query<Option<u16>>,
        hostname: Query<Option<String>>,
    ) -> TardisApiResult<Vec<SgGateway>> {
        let result = GatewayService::list(
            namespace.0.clone(),
            GatewayQueryDto {
                name: name.0,
                namespace: namespace.0,
                port: port.0,
                hostname: hostname.0,
            },
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Gateway
    #[oai(path = "/", method = "post")]
    async fn add(&self, namespace: Query<Option<String>>, gateway: Json<SgGateway>) -> TardisApiResult<SgGateway> {
        TardisResp::ok(GatewayService::add(namespace.0, gateway.0).await?)
    }

    /// Delete Gateway
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, namespace: Query<Option<String>>, name: Query<String>) -> TardisApiResult<Void> {
        GatewayService::delete(namespace.0, &name.0).await?;
        TardisResp::ok(Void {})
    }
}
