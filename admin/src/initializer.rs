use std::sync::Arc;

use crate::api::{gateway_api, plugin_api, route_api};
use crate::client::init_client;
use crate::config::SpacegateAdminConfig;
use crate::constants;
use crate::constants::DOMAIN_CODE;
use tardis::basic::result::TardisResult;
use tardis::web::web_server::{TardisWebServer, WebServerModule};
use tardis::TardisFuns;

pub(crate) async fn init(web_server: &TardisWebServer) -> TardisResult<()> {
    let funs = TardisFuns::inst(DOMAIN_CODE.to_string(), None);
    let config = funs.conf::<SpacegateAdminConfig>();
    init_client(&funs).await?;
    // todo 根据现有的k8s资源初始化成VO
    init_api(config, web_server).await
}

async fn init_api(config: Arc<SpacegateAdminConfig>, web_server: &TardisWebServer) -> TardisResult<()> {
    let module = if let Some(basic_auth) = config.basic_auth {
        WebServerModule::from((gateway_api::GatewayApi, plugin_api::PluginApi, route_api::HttprouteApi)).middleware(basic_auth)
    } else {
        WebServerModule::from((gateway_api::GatewayApi, plugin_api::PluginApi, route_api::HttprouteApi))
    };
    web_server.add_module(constants::DOMAIN_CODE, module).await;
    Ok(())
}
