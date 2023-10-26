use crate::model::query_dto::SgTlsConfigQueryVO;
use crate::model::vo::gateway_vo::SgTlsConfigVO;
use crate::model::vo::Vo;
use crate::service::base_service::BaseService;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use super::gateway_service::GatewayService;

pub struct TlsConfigService;

impl BaseService<'_, SgTlsConfigVO> for TlsConfigService {}

impl TlsConfigService {
    pub(crate) async fn list(query: SgTlsConfigQueryVO) -> TardisResult<Vec<SgTlsConfigVO>> {
        //todo query
        Self::get_type_map()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgTlsConfigVO>>>()
    }

    pub(crate) async fn add(add: SgTlsConfigVO) -> TardisResult<()> {
        Self::add_vo(add).await?;
        Ok(())
    }

    pub(crate) async fn update(update: SgTlsConfigVO) -> TardisResult<()> {
        let unique_name = update.get_unique_name();
        if let Some(o_str) = Self::get_type_map().await?.remove(&unique_name) {
            let mut o: SgTlsConfigVO = serde_json::from_str(&o_str)
                .map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{} failed:{e}", SgTlsConfigVO::get_vo_type(), unique_name), ""))?;
            if let Some(ids) = o.ref_ids {
                for ref_id in ids {
                    GatewayService::update_by_id(&ref_id).await?;
                }
            }
            Self::update_vo(update).await?;
        } else {
        }

        Ok(())
    }

    pub(crate) async fn add_ref_ids(id: &str, ref_ids: &[String]) -> TardisResult<()> {
        let mut ref_ids = ref_ids.to_vec();
        if let Some(o_str) = Self::get_type_map().await?.remove(id) {
            let mut o: SgTlsConfigVO =
                serde_json::from_str(&o_str).map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{id} failed:{e}", SgTlsConfigVO::get_vo_type()), ""))?;
            if let Some(ids) = &mut o.ref_ids {
                ref_ids.append(ids);
                o.ref_ids = Some(ref_ids);
            } else {
                o.ref_ids = Some(ref_ids);
            }
            Self::update_vo(o).await?;
        } else {
            return Err(TardisError::not_found("", ""));
        };
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(&id).await?;
        Ok(())
    }
}
