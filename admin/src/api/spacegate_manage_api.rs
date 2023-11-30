use crate::model::query_dto::{SpacegateInstQueryDto, ToInstance};
use crate::model::vo::spacegate_inst_vo::InstConfigVo;
use crate::service::spacegate_manage_service::SpacegateManageService;
use kernel_common::client::k8s_client::DEFAULT_CLIENT_NAME;
use tardis::basic::error::TardisError;
use tardis::web::poem::session::Session;
use tardis::web::poem_openapi;
use tardis::web::poem_openapi::param::Query;
use tardis::web::poem_openapi::payload::Json;
use tardis::web::web_resp::{TardisApiResult, TardisResp, Void};

#[derive(Clone, Default)]
pub struct SpacegateSelectApi;

#[derive(Clone, Default)]
pub struct SpacegateManageApi;

#[poem_openapi::OpenApi(prefix_path = "/spacegate")]
impl SpacegateSelectApi {
    /// Get select Spacegate Inst
    #[oai(path = "/", method = "get")]
    async fn get(&self, session: &Session) -> TardisApiResult<String> {
        let client_name = &super::get_client_name(session).await;
        if !client_name.is_empty() && client_name != DEFAULT_CLIENT_NAME {
            if SpacegateManageService::check(client_name).await.is_ok() {
                return TardisResp::ok(client_name.to_string());
            }
        }
        session.set("client_name", DEFAULT_CLIENT_NAME);
        TardisResp::ok(DEFAULT_CLIENT_NAME.to_string())
    }

    /// Select Spacegate Inst
    #[oai(path = "/", method = "post")]
    async fn select(&self, name: Query<String>, session: &Session) -> TardisApiResult<Void> {
        if name.0.is_empty() {
            return TardisResp::err(TardisError::bad_request("[admin] select name cannot be empty", ""));
        }
        if name.0 != DEFAULT_CLIENT_NAME {
            SpacegateManageService::check(&name.0).await?;
        }
        session.set("client_name", &name.0);
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
    #[oai(path = "/", method = "delete")]
    async fn delete(&self, name: Query<String>) -> TardisApiResult<Void> {
        SpacegateManageService::delete(&name.0).await?;
        TardisResp::ok(Void {})
    }
}
