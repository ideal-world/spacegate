use crate::api::{gateway_api, plugin_api, route_api};
use crate::client::init_client;
use crate::constants;
use crate::constants::DOMAIN_CODE;
use tardis::basic::result::TardisResult;
use tardis::web::web_server::{TardisWebServer, WebServerModule};
use tardis::TardisFuns;

pub(crate) async fn init(web_server: &TardisWebServer) -> TardisResult<()> {
    let funs = TardisFuns::inst(DOMAIN_CODE.to_string(), None);
    init_client(&funs).await?;
    // todo 根据现有的k8s资源初始化成VO
    init_api(web_server).await
}

async fn init_api(web_server: &TardisWebServer) -> TardisResult<()> {
    web_server
        .add_module(
            constants::DOMAIN_CODE,
            WebServerModule::from((gateway_api::GatewayApi, plugin_api::PluginApi, route_api::HttprouteApi)),
        )
        .await;
    Ok(())
}
