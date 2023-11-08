use crate::model::query_dto::HttpRouteQueryInst;
use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::service::base_service::VoBaseService;
use tardis::basic::result::TardisResult;

pub struct HttpRouteVoService;

impl VoBaseService<SgHttpRouteVo> for HttpRouteVoService {}

impl HttpRouteVoService {
    pub(crate) async fn list(query: HttpRouteQueryInst) -> TardisResult<Vec<SgHttpRouteVo>> {
        let map = Self::get_type_map().await?;
        Ok(
            if query.names.is_none() && query.gateway_name.is_none() && query.hostnames.is_none() && query.filter_ids.is_none() {
                map.into_values().collect()
            } else {
                map.into_values()
                    .into_iter()
                    .filter(|r| {
                        query.names.as_ref().map_or(true, |names| names.iter().any(|n| n.is_match(&r.name)))
                            && query.gateway_name.as_ref().map_or(true, |gateway_name| gateway_name.is_match(&r.gateway_name))
                            && query.hostnames.as_ref().map_or(true, |hostnames| {
                                r.hostnames.as_ref().map_or(false, |r_hostnames| hostnames.iter().any(|hn| r_hostnames.iter().any(|rn| hn.is_match(rn))))
                            })
                            && query.filter_ids.as_ref().map_or(true, |filter_ids| {
                                r.filters.as_ref().map_or(false, |r_filters| filter_ids.iter().any(|f_id| r_filters.iter().any(|rf| f_id.is_match(rf))))
                            })
                    })
                    .collect::<Vec<SgHttpRouteVo>>()
            },
        )
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
