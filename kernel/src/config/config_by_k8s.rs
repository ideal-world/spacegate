use k8s_gateway_api::{Gateway, HttpRoute};
use tardis::basic::result::TardisResult;

use super::{gateway_dto::SgGateway, http_route_dto::SgHttpRoute};

pub async fn init(_namespaces: &str) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    todo!()
}

async fn process_gateway_config(_gateway_objs: Vec<Gateway>) -> TardisResult<Vec<SgGateway>> {
    todo!()
}

async fn process_httpRoute_config(_httpRoute_objs: Vec<HttpRoute>) -> TardisResult<Vec<SgHttpRoute>> {
    todo!()
}
