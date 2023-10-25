use crate::model::query_dto::{PluginQueryDto, SgTlsConfigQueryVO};
use crate::model::vo::backend_vo::BackendRefVO;
use crate::model::vo::gateway_vo::SgTlsConfigVO;
use crate::model::vo::plugin_vo::SgFilterVO;
use crate::service::base_service::BaseService;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

pub struct TlsConfigService;

impl TlsConfigService {
    pub(crate) async fn list(query: SgTlsConfigQueryVO) -> TardisResult<Vec<SgTlsConfigVO>> {
        //todo query
        BaseService::get_type_map::<SgTlsConfigVO>()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgTlsConfigVO>>>()
    }

    pub(crate) async fn add(add: SgTlsConfigVO) -> TardisResult<()> {
        BaseService::add::<SgTlsConfigVO>(add).await?;
        Ok(())
    }
    pub(crate) async fn update(update: SgTlsConfigVO) -> TardisResult<()> {
        BaseService::update::<SgTlsConfigVO>(update).await?;
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        
        BaseService::delete::<SgTlsConfigVO>(&id).await?;
        Ok(())
    }
}
