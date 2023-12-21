use serde::{Deserialize, Serialize};
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::web_resp::{TardisApiResult, TardisResp};

use crate::model::query_dto::{BackendRefQueryInst, GatewayQueryInst, HttpRouteQueryInst, PluginQueryInst};
use crate::service::backend_ref_service::BackendRefVoService;
use crate::service::gateway_service::GatewayVoService;
use crate::{
    model::query_dto::{SgTlsQueryInst, SpacegateInstQueryInst},
    service::{plugin_service::PluginVoService, route_service::HttpRouteVoService, secret_service::TlsVoService, spacegate_manage_service::SpacegateManageService},
};

#[derive(Clone, Default)]
pub struct DashboardApi;

/// Dashboard API
#[poem_openapi::OpenApi(prefix_path = "/dashboard")]
impl DashboardApi {
    /// Get Dashboard Metrics
    #[oai(path = "/statistics", method = "get")]
    async fn statistics(&self, session: &Session) -> TardisApiResult<Statistics> {
        let client_name = &super::get_instance_name(session).await?;
        TardisResp::ok(Statistics {
            backend_count: BackendRefVoService::list(
                client_name,
                BackendRefQueryInst {
                    names: None,
                    namespace: None,
                    hosts: None,
                },
            )
            .await?
            .len() as i64,
            gateway_count: GatewayVoService::list(
                client_name,
                GatewayQueryInst {
                    names: None,
                    port: None,
                    hostname: None,
                    tls_ids: None,
                    filter_ids: None,
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
                    backend_ids: None,
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
            // add default instance
            instance_count: SpacegateManageService::list(SpacegateInstQueryInst { names: None }).await?.len() as i64 + 1,
        })
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, poem_openapi::Object)]
struct Statistics {
    pub backend_count: i64,
    pub gateway_count: i64,
    pub route_count: i64,
    pub plugin_count: i64,
    pub tls_count: i64,
    pub instance_count: i64,
}
