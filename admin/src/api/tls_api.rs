use crate::model::vo::gateway_vo::SgTlsConfigVO;
use crate::model::vo::tls_vo::SgTlsVO;
use crate::service::secret_service::TlsConfigVoService;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::{Path, Query};
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct TlsApi;

/// Tls API
#[poem_openapi::OpenApi(prefix_path = "/tls")]
impl TlsApi {
    /// Get Tls List
    #[oai(path = "/", method = "get")]
    async fn list(&self, ids: Query<Option<String>>, name: Query<Option<String>>, namespace: Query<Option<String>>, code: Query<Option<String>>) -> TardisApiResult<Void> {
        // let _ = PluginService::list(PluginQueryDto {
        //     ids: ids.0.map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>()),
        //     name: name.0,
        //     namespace: namespace.0,
        //     code: code.0,
        //     target: None,
        // })
        // .await;
        TardisResp::ok(Void {})
    }

    /// Add Tls
    #[oai(path = "/", method = "post")]
    async fn add(&self, tls_config: Json<SgTlsVO>) -> TardisApiResult<Void> {
        TlsConfigVoService::add(tls_config.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Tls
    #[oai(path = "/", method = "put")]
    async fn update(&self, tls_config: Json<SgTlsVO>) -> TardisApiResult<Void> {
        TlsConfigVoService::update(tls_config.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Tls
    #[oai(path = "/:backend_id", method = "put")]
    async fn delete(&self, tls_config_id: Path<String>) -> TardisApiResult<Void> {
        TlsConfigVoService::delete(&tls_config_id.0).await?;
        TardisResp::ok(Void {})
    }
}
