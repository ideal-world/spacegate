use std::sync::Arc;

use crate::api::{auth_api, gateway_api, plugin_api, route_api, spacegate_manage_api, tls_api, BasicAuth};
use crate::client::init_client;
use crate::config::SpacegateAdminConfig;

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
    let module = WebServerModule::from((
        gateway_api::GatewayApi,
        plugin_api::PluginApi,
        route_api::HttprouteApi,
        tls_api::TlsApi,
        auth_api::AuthApi,
        spacegate_manage_api::SpacegateManageApi,
    ));
    if let Some(basic_auth) = config.basic_auth.clone() {
        web_server.add_module(DOMAIN_CODE, module.middleware::<BasicAuth>(basic_auth)).await
    } else {
        web_server.add_module(DOMAIN_CODE, module).await
    };
    Ok(())
}
