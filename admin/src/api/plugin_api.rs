use crate::model::add_dto::{SgFilterAddVo, ToVo};
use crate::model::query_dto::{PluginQueryDto, ToInstance};
use crate::model::vo::plugin_vo::SgFilterVo;
use crate::service::plugin_service::PluginVoService;
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::{Path, Query};
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct PluginApi;

#[poem_openapi::OpenApi(prefix_path = "/plugin")]
impl PluginApi {
    /// Get Plugin List
    #[oai(path = "/", method = "get")]
    async fn list(
        &self,
        ids: Query<Option<String>>,
        name: Query<Option<String>>,
        namespace: Query<Option<String>>,
        code: Query<Option<String>>,
        target_name: Query<Option<String>>,
        target_kind: Query<Option<String>>,
        target_namespace: Query<Option<String>>,
        session: &Session,
    ) -> TardisApiResult<Vec<SgFilterVo>> {
        let client_name = &super::get_client_name(session).await;
        let result = PluginVoService::list(
            client_name,
            PluginQueryDto {
                ids: ids.0.map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>()),
                name: name.0,
                namespace: namespace.0,
                code: code.0,
                target_name: target_name.0,
                target_kind: target_kind.0,
                target_namespace: target_namespace.0,
            }
            .to_instance()?,
        )
        .await?;
        TardisResp::ok(result)
    }

    /// Add Plugin
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<SgFilterAddVo>, session: &Session) -> TardisApiResult<SgFilterVo> {
        let client_name = &super::get_client_name(session).await;
        TardisResp::ok(PluginVoService::add(client_name, add.0.to_vo()?).await?)
    }

    /// Update Plugin
    #[oai(path = "/", method = "put")]
    async fn update(&self, update: Json<SgFilterVo>, session: &Session) -> TardisApiResult<SgFilterVo> {
        let client_name = &super::get_client_name(session).await;
        TardisResp::ok(PluginVoService::update(client_name, update.0).await?)
    }

    /// Delete Plugin
    #[oai(path = "/:plugin_id", method = "put")]
    async fn delete(&self, plugin_id: Path<String>, session: &Session) -> TardisApiResult<Void> {
        let client_name = &super::get_client_name(session).await;
        PluginVoService::delete(client_name, &plugin_id.0).await?;
        TardisResp::ok(Void {})
    }
}
