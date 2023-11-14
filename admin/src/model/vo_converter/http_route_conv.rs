use crate::model::vo::http_route_vo::SgHttpRouteVo;
use crate::model::vo_converter::VoConv;
use kernel_common::inner_model::http_route::SgHttpRoute;
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;

#[async_trait]
impl VoConv<SgHttpRoute, SgHttpRouteVo> for SgHttpRouteVo {
    async fn to_model(self) -> TardisResult<SgHttpRoute> {
        todo!()
    }

    async fn from_model(model: SgHttpRoute) -> TardisResult<SgHttpRouteVo> {
        todo!()
    }
}
