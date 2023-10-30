use crate::model::query_dto::BackendRefQueryDto;
use crate::model::vo::backend_vo::SgBackendRefVO;
use crate::service::base_service::VoBaseService;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct BackendRefVoService;

impl VoBaseService<SgBackendRefVO> for BackendRefVoService {}

impl BackendRefVoService {
    pub(crate) async fn list(id: Option<String>, query: BackendRefQueryDto) -> TardisResult<Vec<SgBackendRefVO>> {
        //todo query
        Self::get_str_type_map()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgBackendRefVO>>>()
    }

    pub(crate) async fn add(add: SgBackendRefVO) -> TardisResult<()> {
        Self::add_vo(add).await?;
        Ok(())
    }
    pub(crate) async fn update(update: SgBackendRefVO) -> TardisResult<()> {
        Self::update_vo(update).await?;
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(&id).await?;
        Ok(())
    }
}
