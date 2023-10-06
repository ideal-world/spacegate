use crate::dto::base_dto::CommonPageDto;
use crate::dto::query_dto::PluginQueryDto;
use crate::service::plugin_service::PluginService;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct PluginApi;

#[poem_openapi::OpenApi(prefix_path = "/plugin")]
impl PluginApi {
    /// Get Plugin List
    #[oai(path = "/", method = "get")]
    async fn list(&self, ids: Query<Option<String>>, name: Query<Option<String>>, namespace: Query<Option<String>>, code: Query<Option<String>>) -> TardisApiResult<Void> {
        let _ = PluginService::list(PluginQueryDto {
            ids: ids.0.map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>()),
            name: name.0,
            namespace: namespace.0,
            code: code.0,
            target: None,
        })
        .await;
        TardisResp::ok(Void {})
    }
}
