use crate::model::query_dto::BackendRefQueryDto;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::service::base_service::VoBaseService;
use tardis::basic::result::TardisResult;

pub struct HttpRouteVoService;

impl VoBaseService<SgHttpRouteVo> for HttpRouteVoService {}

impl HttpRouteVoService {
    pub(crate) async fn list(id: Option<String>, query: BackendRefQueryDto) -> TardisResult<Vec<SgHttpRouteVo>> {
        //todo query
        Ok(Self::get_type_map().await?.into_values().into_iter().collect::<Vec<SgHttpRouteVo>>())
    }

    pub(crate) async fn add(add: SgHttpRouteVo) -> TardisResult<()> {
        Self::add_vo(add).await?;
        Ok(())
    }
    pub(crate) async fn update(update: SgHttpRouteVo) -> TardisResult<()> {
        Self::update_vo(update).await?;
        Ok(())
    }

    pub(crate) async fn delete(id: &str) -> TardisResult<()> {
        Self::delete_vo(&id).await?;
        Ok(())
    }
}
