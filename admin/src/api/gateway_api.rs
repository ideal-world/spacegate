use crate::model::query_dto::{GatewayQueryDto, ToInstance};
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::service::gateway_service::GatewayVoService;
use tardis::basic::error::TardisError;
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::{Path, Query};
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
        filter_ids: Query<Option<String>>,
        session: &Session,
    ) -> TardisApiResult<Vec<SgGatewayVo>> {
        let client_name = &super::get_instance_name(session).await?;
        let result = GatewayVoService::list(
            client_name,
            GatewayQueryDto {
                names: names.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                port: port.0.map(|p| p.parse::<u16>()).transpose().map_err(|_e| TardisError::bad_request("bad port format", ""))?,
                hostname: hostname.0,
                tls_ids: tls_ids.0.map(|tls_ids| tls_ids.split(',').map(|tls_id| tls_id.to_string()).collect()),
                filter_ids: filter_ids.0.map(|filter_ids| filter_ids.split(',').map(|filter_id| filter_id.to_string()).collect()),
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Gateway
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgGatewayVo>, session: &Session) -> TardisApiResult<SgGatewayVo> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(GatewayVoService::add(client_name, add.0).await?)
    }

    /// Update Gateway
    #[oai(path = "/", method = "put")]
    async fn update(&self, update: Json<SgGatewayVo>, session: &Session) -> TardisApiResult<SgGatewayVo> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(GatewayVoService::update(client_name, update.0).await?)
    }

    /// Delete Gateway
    #[oai(path = "/:name", method = "delete")]
    async fn delete(&self, name: Path<String>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_instance_name(session).await?;
        GatewayVoService::delete(client_name, &name.0).await?;
        TardisResp::ok(Void {})
    }
}
