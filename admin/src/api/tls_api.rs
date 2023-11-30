use crate::model::query_dto::{SgTlsQueryVO, ToInstance};
use crate::service::secret_service::TlsVoService;
use kernel_common::inner_model::gateway::SgTls;
use tardis::web::poem::session::Session;
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
    async fn list(&self, names: Query<Option<String>>, session: &Session) -> TardisApiResult<Vec<SgTls>> {
        let client_name = &super::get_instance_name(session).await?;
        let result = TlsVoService::list(
            client_name,
            SgTlsQueryVO {
                names: names.0.map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>()),
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Tls
    #[oai(path = "/", method = "post")]
    async fn add(&self, tls_config: Json<SgTls>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_instance_name(session).await?;
        TlsVoService::add(client_name, tls_config.0).await?;
        TardisResp::ok(Void {})
    }

    /// Update Tls
    #[oai(path = "/", method = "put")]
    async fn update(&self, tls_config: Json<SgTls>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_instance_name(session).await?;
        TlsVoService::update(client_name, tls_config.0).await?;
        TardisResp::ok(Void {})
    }

    /// Delete Tls
    #[oai(path = "/:backend_id", method = "put")]
    async fn delete(&self, tls_config_id: Path<String>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_instance_name(session).await?;
        TlsVoService::delete(client_name, &tls_config_id.0).await?;
        TardisResp::ok(Void {})
    }
}
