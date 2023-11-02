use crate::model::query_dto::GatewayQueryDto;
use crate::model::vo::gateway_vo::SgHttpRouteVo;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::service::gateway_service::HttpRouteVoService;
use crate::service::route_service::HttpRouteVoService;
use tardis::basic::error::TardisError;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct HttprouteApi;

#[poem_openapi::OpenApi(prefix_path = "/httproute")]
impl HttprouteApi {
    /// Get Httproute List
    #[oai(path = "/", method = "get")]
    async fn list(&self, names: Query<Option<Vec<String>>>, port: Query<Option<String>>, hostname: Query<Option<String>>) -> TardisApiResult<Vec<SgHttpRouteVo>> {
        let result = HttpRouteVoService::list(
            "",
            GatewayQueryDto {
                names: names.0,
                port: port.0.map(|p| p.parse::<u16>()).transpose().map_err(|e| TardisError::bad_request("bad port format", ""))?,
                hostname: hostname.0,
            },
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Httproute
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgHttpRouteVo>) -> TardisApiResult<Void> {
        HttpRouteVoService::add(add.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Httproute
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgHttpRouteVo>) -> TardisApiResult<Void> {
        HttpRouteVoService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Httproute
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        HttpRouteVoService::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
