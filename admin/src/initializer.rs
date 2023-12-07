use crate::api::{auth_api, backend_api, dashboard_api, gateway_api, plugin_api, route_api, spacegate_manage_api, tls_api, BasicAuth, CookieMW};
use crate::client::init_client;

use crate::constants::DOMAIN_CODE;
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::gateway_service::GatewayVoService;
use k8s_gateway_api::{Gateway, HttpRoute};
use kernel_common::client::{cache_client, k8s_client};
use kernel_common::constants::k8s_constants::GATEWAY_CLASS_NAME;
use kernel_common::helper::k8s_helper::WarpKubeResult;
use kernel_common::inner_model::gateway::SgGateway;
use kernel_common::inner_model::http_route::SgHttpRoute;
use kernel_common::k8s_crd::http_spaceroute::{self, HttpSpaceroute};
use kernel_common::k8s_crd::sg_filter::SgFilter;
use kube::api::ListParams;
use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use tardis::futures_util::future::join_all;
use tardis::web::web_server::{TardisWebServer, WebServerModule};
use tardis::TardisFuns;

use crate::{
    model::query_dto::{SgTlsQueryInst, SpacegateInstQueryInst},
    service::{plugin_service::PluginVoService, route_service::HttpRouteVoService, secret_service::TlsVoService, spacegate_manage_service::SpacegateManageService},
};

pub(crate) async fn init(web_server: &TardisWebServer) -> TardisResult<()> {
    let funs = TardisFuns::inst(DOMAIN_CODE.to_string(), None);
    init_client(&funs).await?;
    init_spacegate_to_config().await?;
    init_api(web_server).await
}

/// Initialized to VO based on existing instance
async fn init_spacegate_to_config() -> TardisResult<()> {
    for inst_vo in SpacegateManageService::list(SpacegateInstQueryInst { names: None }).await? {
        let client_name_string = inst_vo.get_unique_name();
        let client_name = client_name_string.as_str();
        let (gateway_models, http_spaceroute_models) = match inst_vo.type_ {
            crate::model::vo::spacegate_inst_vo::InstConfigType::K8sClusterConfig => {
                let (gateway_api, http_spaceroute_api, http_route_api): (Api<Gateway>, Api<HttpSpaceroute>, Api<HttpRoute>) = (
                    Api::all((*k8s_client::get(client_name).await?).clone()),
                    Api::all((*k8s_client::get(client_name).await?).clone()),
                    Api::all((*k8s_client::get(client_name).await?).clone()),
                );
                let gateway_objs = gateway_api
                    .list(&ListParams::default())
                    .await
                    .warp_result()?
                    .into_iter()
                    .filter(|gateway_obj| gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME)
                    .collect::<Vec<Gateway>>();
                let gateway_models =
                    join_all(gateway_objs.into_iter().map(|gateway_obj| async move { return SgGateway::from_kube_gateway(client_name, gateway_obj).await }).collect::<Vec<_>>())
                        .await
                        .into_iter()
                        .collect::<TardisResult<Vec<_>>>()?;
                let gateway_uniques = gateway_models.iter().map(|gateway_config| gateway_config.name.clone()).collect::<Vec<String>>();

                let http_route_objs: Vec<HttpSpaceroute> = http_spaceroute::get_http_spaceroute_by_api(&gateway_uniques, (&http_spaceroute_api, &http_route_api)).await?;

                let http_route_models =
                    join_all(http_route_objs.into_iter().map(|http_route_obj| return SgHttpRoute::from_kube_httpspaceroute(client_name, http_route_obj)).collect::<Vec<_>>())
                        .await
                        .into_iter()
                        .collect::<TardisResult<Vec<_>>>()?;
                (gateway_models, http_route_models)
            }
            crate::model::vo::spacegate_inst_vo::InstConfigType::RedisConfig => {
                let redis_client = cache_client::get(&client_name).await?;

                let gateway_configs = redis_client.hgetall(cache_client::CONF_GATEWAY_KEY).await?;
                if gateway_configs.is_empty() {
                    return Err(TardisError::not_found(
                        &format!("[Admin.Init] Gateway Config not found in {}", cache_client::CONF_GATEWAY_KEY),
                        "",
                    ));
                }
                let gateway_models = gateway_configs
                    .into_values()
                    .map(|v| {
                        tardis::TardisFuns::json.str_to_obj::<SgGateway>(&v).map_err(|e| TardisError::format_error(&format!("[SG.Config] Gateway Config parse error {}", e), ""))
                    })
                    .collect::<TardisResult<Vec<SgGateway>>>()?;

                let http_route_models = Vec::new();
                for gateway_model in &gateway_models {
                    let http_route_configs = redis_client.lrangeall(&format!("{}{}", cache_client::CONF_HTTP_ROUTE_KEY, gateway_model.name)).await?;
                    let http_route_configs = http_route_configs
                        .into_iter()
                        .map(|v| {
                            tardis::TardisFuns::json
                                .str_to_obj::<SgHttpRoute>(&v)
                                .map_err(|e| TardisError::format_error(&format!("[SG.Config] Http Route Config parse error {}", e), ""))
                        })
                        .collect::<TardisResult<Vec<SgHttpRoute>>>()?;
                }
                (gateway_models, http_route_models)
            }
        };

        //Add gatewayVo
        for gateway_model in gateway_models {
            let add_model = SgGatewayVo::from_model(gateway_model).await?;
            let _ = GatewayVoService::add(client_name, add_model).await;
        }

        //Add httprouteVO
        for http_spaceroute_model in http_spaceroute_models {
            let add_model = SgHttpRouteVo::from_model(http_spaceroute_model).await?;
            let _ = HttpRouteVoService::add_vo(client_name, add_model).await;
        }
    }
    Ok(())
}

async fn init_api(web_server: &TardisWebServer) -> TardisResult<()> {
    let module = WebServerModule::from((
        backend_api::BackendApi,
        dashboard_api::DashboardApi,
        gateway_api::GatewayApi,
        plugin_api::PluginApi,
        route_api::HttprouteApi,
        tls_api::TlsApi,
        auth_api::AuthApi,
        spacegate_manage_api::SpacegateManageApi,
        spacegate_manage_api::SpacegateSelectApi,
    ))
    .middleware((BasicAuth, CookieMW));

    web_server.add_module(DOMAIN_CODE, module).await;

    Ok(())
}
