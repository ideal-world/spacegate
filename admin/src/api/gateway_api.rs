use crate::model::query_dto::{GatewayQueryDto, ToInstance};
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::service::gateway_service::GatewayVoService;
use tardis::basic::error::TardisError;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct GatewayApi;

/// Gateway API
#[poem_openapi::OpenApi(prefix_path = "/gateway")]
impl GatewayApi {
    /// Get Gateway List
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        names: Query<Option<String>>,
        port: Query<Option<String>>,
        hostname: Query<Option<String>>,
        tls_ids: Query<Option<String>>,
    ) -> TardisApiResult<Vec<SgGatewayVo>> {
        let result = GatewayVoService::list(
            GatewayQueryDto {
                names: names.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                port: port.0.map(|p| p.parse::<u16>()).transpose().map_err(|e| TardisError::bad_request("bad port format", ""))?,
                hostname: hostname.0,
                tls_ids: tls_ids.0.map(|tls_ids| tls_ids.split(',').map(|tls_id| tls_id.to_string()).collect()),
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Gateway
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgGatewayVo>) -> TardisApiResult<Void> {
        GatewayVoService::add(add.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Gateway
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgGatewayVo>) -> TardisApiResult<Void> {
        GatewayVoService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Gateway
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        GatewayVoService::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
