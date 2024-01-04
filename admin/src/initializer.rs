use std::mem;

use crate::api::{auth_api, backend_api, dashboard_api, gateway_api, plugin_api, route_api, spacegate_manage_api, tls_api, BasicAuth, CookieMW};
use crate::client::init_client;

use crate::constants::DOMAIN_CODE;
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo::spacegate_inst_vo::InstConfigType;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::backend_ref_service::BackendRefVoService;
use crate::service::base_service::VoBaseService;
use crate::service::gateway_service::GatewayVoService;
use itertools::Itertools;
use k8s_gateway_api::{Gateway, HttpRoute};
use kernel_common::client::{cache_client, k8s_client};
use kernel_common::constants::k8s_constants::GATEWAY_CLASS_NAME;
use kernel_common::helper::k8s_helper::WarpKubeResult;
use kernel_common::inner_model::gateway::SgGateway;
use kernel_common::inner_model::http_route::SgHttpRoute;
use kernel_common::k8s_crd::http_spaceroute::{self, HttpSpaceroute};
use kube::api::ListParams;
use kube::Api;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use tardis::futures_util::future::join_all;
use tardis::web::web_server::{TardisWebServer, WebServerModule};
use tardis::{log, TardisFuns};

use crate::{
    model::query_dto::SpacegateInstQueryInst,
    service::{plugin_service::PluginVoService, route_service::HttpRouteVoService, secret_service::TlsVoService, spacegate_manage_service::SpacegateManageService},
};

pub async fn init(web_server: &TardisWebServer) -> TardisResult<()> {
    let funs = TardisFuns::inst(DOMAIN_CODE.to_string(), None);
    init_client(&funs).await?;
    init_spacegate_to_config().await?;
    init_api(web_server).await
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

/// Initialized to VO based on existing instance
async fn init_spacegate_to_config() -> TardisResult<()> {
    let mut client_name_types =
        SpacegateManageService::list(SpacegateInstQueryInst { names: None }).await?.into_iter().map(|inst_vo| (inst_vo.get_unique_name(), inst_vo.type_)).collect::<Vec<_>>();
    //add default
    client_name_types.insert(
        0,
        (
            k8s_client::DEFAULT_CLIENT_NAME.to_string(),
            if crate::client::get_base_is_kube().await? {
                InstConfigType::K8sClusterConfig
            } else {
                InstConfigType::RedisConfig
            },
        ),
    );

    for (client_name_string, clinet_type) in client_name_types {
        init_config_by_single_client(&client_name_string, clinet_type).await?;
    }
    Ok(())
}

pub async fn init_config_by_single_client(client_name: &str, clinet_type: InstConfigType) -> TardisResult<()> {
    log::info!("[Admin.Init] Start [{client_name}] Init....");
    let (gateway_models, http_spaceroute_models) = match clinet_type {
        InstConfigType::K8sClusterConfig => {
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
                join_all(gateway_objs.into_iter().map(|gateway_obj| async move { SgGateway::from_kube_gateway(client_name, gateway_obj).await }).collect::<Vec<_>>())
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
        InstConfigType::RedisConfig => {
            let redis_client = cache_client::get(client_name).await?;

            let gateway_configs = redis_client.hgetall(cache_client::CONF_GATEWAY_KEY).await?;
            if gateway_configs.is_empty() {
                log::info!("[Admin.Init] No Gateway Config found. initializing skip");
                return Ok(());
            }
            let gateway_models = gateway_configs
                .into_values()
                .map(|v| tardis::TardisFuns::json.str_to_obj::<SgGateway>(&v).map_err(|e| TardisError::format_error(&format!("[Admin.Init] Gateway Config parse error {}", e), "")))
                .collect::<TardisResult<Vec<SgGateway>>>()?;

            let mut http_route_models = Vec::new();
            for gateway_model in &gateway_models {
                let http_route_configs = redis_client.lrangeall(&format!("{}{}", cache_client::CONF_HTTP_ROUTE_KEY, gateway_model.name)).await?;
                let mut http_route_configs = http_route_configs
                    .into_iter()
                    .map(|v| {
                        tardis::TardisFuns::json
                            .str_to_obj::<SgHttpRoute>(&v)
                            .map_err(|e| TardisError::format_error(&format!("[Admin.Init] Http Route Config parse error {}", e), ""))
                    })
                    .collect::<TardisResult<Vec<SgHttpRoute>>>()?;
                http_route_models.append(&mut http_route_configs)
            }
            (gateway_models, http_route_models)
        }
    };

    let mut add_filter_models = Vec::new();

    //Add gatewayVo
    for gateway_model in gateway_models {
        let mut add_model = SgGatewayVo::from_model(gateway_model).await?;
        let add_name = add_model.get_unique_name();

        let add_tls_vec = mem::take(&mut add_model.tls_vos);
        add_filter_models.append(&mut mem::take(&mut add_model.filter_vos));

        for add_tls_model in add_tls_vec.into_iter().unique() {
            if TlsVoService::add_vo(client_name, add_tls_model).await.is_ok() {
                log::info!("[Admin.Init] Add TlsVo [{}]", add_name);
            }
        }

        if GatewayVoService::add_vo(client_name, add_model).await.is_ok() {
            log::info!("[Admin.Init] Add GatewayVo [{}]", add_name);
        };
    }

    //Add httprouteVO
    for http_spaceroute_model in http_spaceroute_models {
        let mut add_model = SgHttpRouteVo::from_model(http_spaceroute_model).await?;
        let add_name = add_model.get_unique_name();

        let add_backend_vos = mem::take(&mut add_model.backend_vos);
        add_filter_models.append(&mut mem::take(&mut add_model.filter_vos));
        if HttpRouteVoService::add_vo(client_name, add_model).await.is_ok() {
            log::info!("[Admin.Init] Add HttpRouteVo [{}]", add_name);
        }

        for add_backend_vo in add_backend_vos.into_iter().unique() {
            let add_name = add_backend_vo.get_unique_name();
            if BackendRefVoService::add_vo(client_name, add_backend_vo).await.is_ok() {
                log::info!("[Admin.Init] Add BackendRefVo [{}]", add_name);
            }
        }
    }

    //Add filter
    for add_filter_model in add_filter_models.into_iter().unique() {
        let add_name = add_filter_model.get_unique_name();
        if PluginVoService::add_vo(client_name, add_filter_model).await.is_ok() {
            log::info!("[Admin.Init] Add FilterVo [{}]", add_name);
        }
    }
    log::info!("[Admin.Init] Init [{client_name}] success");
    Ok(())
}
