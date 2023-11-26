use crate::model::query_dto::{HttpRouteQueryDto, ToInstance};
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::service::route_service::HttpRouteVoService;
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
    async fn list(
        &self,
        names: Query<Option<String>>,
        gateway_name: Query<Option<String>>,
        hostnames: Query<Option<String>>,
        filter_ids: Query<Option<String>>,
    ) -> TardisApiResult<Vec<SgHttpRouteVo>> {
        //todo client_name
        let client_name = "";
        let result = HttpRouteVoService::list(
            client_name,
            HttpRouteQueryDto {
                names: names.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                gateway_name: gateway_name.0,
                hostnames: hostnames.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
                filter_ids: filter_ids.0.map(|n| n.split(',').map(|n| n.to_string()).collect()),
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Httproute
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgHttpRouteVo>) -> TardisApiResult<Void> {
        //todo client_name
        let client_name = "";
        HttpRouteVoService::add(client_name, add.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Httproute
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<SgHttpRouteVo>) -> TardisApiResult<Void> {
        //todo client_name
        let client_name = "";
        HttpRouteVoService::update(client_name, backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Httproute
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        //todo client_name
        let client_name = "";
        HttpRouteVoService::delete(client_name, &name.0).await?;
        TardisResp::ok(Void {})
    }
}
