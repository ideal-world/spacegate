use crate::model::query_dto::BackendRefQueryDto;
use crate::model::vo::backend_vo::BackendRefVO;
use crate::service::base_service::BaseService;
use std::process::id;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;
use tardis::web::poem::delete;

pub struct BackendRefService;

impl BackendRefService {
    pub(crate) async fn list(id: Option<String>, query: BackendRefQueryDto) -> TardisResult<Vec<BackendRefVO>> {
        //todo query
        BaseService::get_type_map::<BackendRefVO>()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<BackendRefVO>>>()
    }

    pub(crate) async fn add(add: BackendRefVO) -> TardisResult<()> {
        BaseService::add::<BackendRefVO>(add).await?;
        Ok(())
    }
    pub(crate) async fn update(update: BackendRefVO) -> TardisResult<()> {
        BaseService::update::<BackendRefVO>(update).await?;
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        BaseService::delete::<BackendRefVO>(&id).await?;
        Ok(())
    }
}
