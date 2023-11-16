use crate::model::query_dto::BackendRefQueryInst;
use crate::model::vo::backend_vo::SgBackendRefVo;

use crate::service::base_service::VoBaseService;

use tardis::basic::result::TardisResult;

pub struct BackendRefVoService;

impl VoBaseService<SgBackendRefVo> for BackendRefVoService {}

impl BackendRefVoService {
    pub(crate) async fn list(query: BackendRefQueryInst) -> TardisResult<Vec<SgBackendRefVo>> {
        Ok(Self::get_type_map()
            .await?
            .into_values()
            .filter(|b|
                if let Some(q_names) = &query.names {
                    q_names.iter().any(|q| q.is_match(&b.name_or_host))
                } else {
                    true
                } &&
                    if let Some(namespace) = &query.namespace {
                        if let Some(b_namespace)=&b.namespace{
                            namespace.is_match(b_namespace)
                        }
                        else { false }
                } else {
                    true
                }
            )
            .collect())
    }

    pub(crate) async fn add(add: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        Self::add_vo(add).await
    }
    pub(crate) async fn update(update: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        Self::update_vo(update).await
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(id).await?;
        Ok(())
    }
}
