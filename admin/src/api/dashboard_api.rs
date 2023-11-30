use serde::{Deserialize, Serialize};
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::web_resp::{TardisApiResult, TardisResp};

use crate::model::query_dto::{GatewayQueryInst, HttpRouteQueryInst, PluginQueryInst};
use crate::service::gateway_service::GatewayVoService;
use crate::{
    model::query_dto::{SgTlsQueryInst, SpacegateInstQueryInst},
    service::{plugin_service::PluginVoService, route_service::HttpRouteVoService, secret_service::TlsVoService, spacegate_manage_service::SpacegateManageService},
};

#[derive(Clone, Default)]
pub struct DashboardApi;

/// Dashboard API
#[poem_openapi::OpenApi(prefix_path = "/")]
impl DashboardApi {
    #[oai(path = "/", method = "get")]
    async fn statistics(&self, session: &Session) -> TardisApiResult<Statistics> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(Statistics {
            gateway_count: GatewayVoService::list(
                client_name,
                GatewayQueryInst {
                    names: None,
                    port: None,
                    hostname: None,
                    tls_ids: None,
                },
            )
            .await?
            .len() as i64,
            route_count: HttpRouteVoService::list(
                client_name,
                HttpRouteQueryInst {
                    names: None,
                    gateway_name: None,
                    hostnames: None,
                    filter_ids: None,
                },
            )
            .await?
            .len() as i64,
            plugin_count: PluginVoService::list(
                client_name,
                PluginQueryInst {
                    ids: None,
                    name: None,
                    code: None,
                    namespace: None,
                    target_name: None,
                    target_kind: None,
                    target_namespace: None,
                },
            )
            .await?
            .len() as i64,
            tls_count: TlsVoService::list(client_name, SgTlsQueryInst { names: None }).await?.len() as i64,
            instance_count: SpacegateManageService::list(SpacegateInstQueryInst { names: None }).await?.len() as i64,
        })
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
struct Statistics {
    pub gateway_count: i64,
    pub route_count: i64,
    pub plugin_count: i64,
    pub tls_count: i64,
    pub instance_count: i64,
}
