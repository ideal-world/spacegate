use crate::model::query_dto::BackendRefQueryInst;
use crate::model::vo::backend_vo::SgBackendRefVo;

use crate::service::base_service::VoBaseService;

use tardis::basic::result::TardisResult;

pub struct BackendRefVoService;

impl VoBaseService<SgBackendRefVo> for BackendRefVoService {}

impl BackendRefVoService {
    pub(crate) async fn list(client_name: &str, query: BackendRefQueryInst) -> TardisResult<Vec<SgBackendRefVo>> {
        Ok(Self::get_type_map(client_name)
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

    pub(crate) async fn add(client_name: &str, add: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        Self::add_vo(client_name, add).await
    }
    pub(crate) async fn update(client_name: &str, update: SgBackendRefVo) -> TardisResult<SgBackendRefVo> {
        Self::update_vo(client_name, update).await
    }

    pub(crate) async fn delete(client_name: &str, id: &str) -> TardisResult<()> {
        Self::delete_vo(client_name, id).await?;
        Ok(())
    }
}
