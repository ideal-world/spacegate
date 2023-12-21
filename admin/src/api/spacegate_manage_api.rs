use crate::model::query_dto::{SpacegateInstQueryDto, ToInstance};
use crate::model::vo::spacegate_inst_vo::InstConfigVo;
use crate::service::spacegate_manage_service::SpacegateManageService;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use tardis::basic::error::TardisError;
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::{Path, Query};
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

use super::SessionInstance;

#[derive(Clone, Default)]
pub struct SpacegateSelectApi;

#[derive(Clone, Default)]
pub struct SpacegateManageApi;

#[poem_openapi::OpenApi(prefix_path = "/spacegate")]
impl SpacegateSelectApi {
    /// Get select Spacegate Inst
    #[oai(path = "/", method = "get")]
    async fn get(&self, session: &Session) -> TardisApiResult<SessionInstance> {
        let instance = SpacegateManageService::get_instance(session).await?;
        if !instance.name.is_empty() && instance.name != DEFAULT_CLIENT_NAME && SpacegateManageService::check(&instance.name).await.is_ok() {
            return TardisResp::ok(instance);
        }
        TardisResp::ok(SpacegateManageService::set_instance_name(DEFAULT_CLIENT_NAME, session).await?)
    }

    /// Select Spacegate Inst
    #[oai(path = "/", method = "post")]
    async fn select(&self, name: Query<String>, session: &Session) -> TardisApiResult<Void> {
        SpacegateManageService::set_instance_name(&name.0, session).await?;
        TardisResp::ok(Void {})
    }
}

#[poem_openapi::OpenApi(prefix_path = "/spacegate/manage")]
impl SpacegateManageApi {
    /// List Spacegate Inst
    #[oai(path = "/", method = "get")]
    async fn list(&self, names: Query<Option<String>>) -> TardisApiResult<Vec<InstConfigVo>> {
        TardisResp::ok(
            SpacegateManageService::list(
                SpacegateInstQueryDto {
                    names: names.0.map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<String>>()),
                }
                .to_instance()?,
            )
            .await?,
        )
    }

    /// Add Spacegate Inst
    #[oai(path = "/", method = "post")]
    async fn add(&self, add: Json<InstConfigVo>) -> TardisApiResult<InstConfigVo> {
        TardisResp::ok(SpacegateManageService::add(add.0).await?)
    }

    /// Update Spacegate Inst
    #[oai(path = "/", method = "put")]
    async fn update(&self, update: Json<InstConfigVo>) -> TardisApiResult<InstConfigVo> {
        TardisResp::ok(SpacegateManageService::update(update.0).await?)
    }

    /// Delete Spacegate Inst
    #[oai(path = "/:name", method = "delete")]
    async fn delete(&self, name: Path<String>, session: &Session) -> TardisApiResult<Void> {
        let selected_client = super::get_instance_name(session).await?;
        if name.0 == selected_client {
            return TardisResp::err(TardisError::bad_request(
                &format!("[Admin.service] not allow to delete selected client {}, please select another before delete", name.0),
                "",
            ));
        }
        SpacegateManageService::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
