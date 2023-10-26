use crate::model::vo::plugin_vo::SgFilterVO;
use crate::model::vo_converter::VoConv;
use kernel_common::inner_model::plugin_filter::SgRouteFilter;
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;

#[async_trait]
impl VoConv<SgRouteFilter, SgFilterVO> for SgFilterVO {
    async fn to_model(self) -> TardisResult<SgRouteFilter> {
        Ok(SgRouteFilter {
            code: self.code,
            name: self.name,
            spec: self.spec,
        })
    }

    async fn from_model(model: SgRouteFilter) -> TardisResult<SgFilterVO> {
        todo!()
    }
}
