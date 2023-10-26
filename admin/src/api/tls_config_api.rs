use crate::model::query_dto::PluginQueryDto;
use crate::model::vo::backend_vo::BackendRefVO;
use crate::service::plugin_service::PluginService;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Path;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct TlsConfigApi;

/// TlsConfig API
#[poem_openapi::OpenApi(prefix_path = "/tlsConfig")]
impl TlsConfigApi {
    /// Get TlsConfig List
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

    /// Add TlsConfig
    #[oai(path = "/", method = "post")]
    async fn add(&self, backend: Json<BackendRefVO>) -> TardisApiResult<Void> {
        PluginService::add(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update TlsConfig
    #[oai(path = "/", method = "put")]
    async fn update(&self, backend: Json<BackendRefVO>) -> TardisApiResult<Void> {
        PluginService::update(backend.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete TlsConfig
    #[oai(path = "/:backend_id", method = "put")]
    async fn delete(&self, backend_id: Path<String>) -> TardisApiResult<Void> {
        PluginService::delete(&backend_id.0).await?;
        TardisResp::ok(Void {})
    }
}
