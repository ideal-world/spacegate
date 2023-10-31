use crate::model::vo::tls_vo::SgTlsVO;
use crate::model::vo_converter::VoConv;
use kernel_common::inner_model::gateway::SgTls;
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;

#[async_trait]
impl VoConv<SgTls, SgTlsVO> for SgTlsVO {
    async fn to_model(self) -> TardisResult<SgTls> {
        Ok(SgTls {
            name: self.name,
            key: self.key,
            cert: self.cert,
        })
    }

    async fn from_model(model: SgTls) -> TardisResult<SgTlsVO> {
        Ok(SgTlsVO {
            name: model.name,
            key: model.key,
            cert: model.cert,
        })
    }
}
