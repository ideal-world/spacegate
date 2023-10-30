use crate::model::query_dto::SgTlsConfigQueryVO;
use crate::model::vo::gateway_vo::SgTlsConfigVO;
use crate::model::vo::Vo;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use super::gateway_service::GatewayVoService;

pub struct TlsConfigVoService;

impl VoBaseService<SgTlsConfigVO> for TlsConfigVoService {}

impl TlsConfigVoService {
    pub(crate) async fn list(query: SgTlsConfigQueryVO) -> TardisResult<Vec<SgTlsConfigVO>> {
        //todo query
        Self::get_str_type_map()
            .await?
            .values()
            .into_iter()
            .map(|v| serde_json::from_str(v).map_err(|e| TardisError::bad_request(&format!(""), "")))
            .collect::<TardisResult<Vec<SgTlsConfigVO>>>()
    }

    pub(crate) async fn add(add: SgTlsConfigVO) -> TardisResult<()> {
        let add_model = add.clone().to_model().await?;
        #[cfg(feature = "k8s")]
        {}
        Self::add_vo(add).await?;
        Ok(())
    }

    pub(crate) async fn update(update: SgTlsConfigVO) -> TardisResult<()> {
        let unique_name = update.get_unique_name();
        if let Some(o_str) = Self::get_str_type_map().await?.remove(&unique_name) {
            let mut o: SgTlsConfigVO = serde_json::from_str(&o_str)
                .map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{} failed:{e}", SgTlsConfigVO::get_vo_type(), unique_name), ""))?;
            if let Some(ids) = o.ref_ids {
                for ref_id in ids {
                    GatewayVoService::update_by_id(&ref_id).await?;
                }
            }
            Self::update_vo(update).await?;
        } else {
        }

        Ok(())
    }

    /// delete:true means delete, false means add
    pub(crate) async fn modify_ref_ids(id: &str, ref_id: &str, delete: bool) -> TardisResult<()> {
        if let Some(o_str) = Self::get_str_type_map().await?.remove(id) {
            let mut o: SgTlsConfigVO =
                serde_json::from_str(&o_str).map_err(|e| TardisError::bad_request(&format!("[SG.admin] Deserialization {}:{id} failed:{e}", SgTlsConfigVO::get_vo_type()), ""))?;
            if let Some(ids) = &mut o.ref_ids {
                if delete {
                    ids.retain(|id| id != ref_id);
                } else {
                    ids.push(ref_id.to_string());
                }
            } else {
                if delete {
                    return Err(TardisError::not_found("delete failed", ""));
                } else {
                    o.ref_ids = Some(vec![ref_id.to_string()]);
                }
            }
            Self::update_vo(o).await?;
        } else {
            return Err(TardisError::not_found(&format!("can not find tls:{id}"), ""));
        };
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(&id).await?;
        Ok(())
    }
}
