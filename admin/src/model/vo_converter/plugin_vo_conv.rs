use crate::model::query_dto::{PluginQueryDto, ToInstance};
use crate::model::vo::plugin_vo::SgFilterVo;
use crate::model::vo_converter::VoConv;
use crate::service::plugin_service::PluginVoService;
use kernel_common::inner_model::plugin_filter::SgRouteFilter;
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

pub struct SgFilterVoConv {}

impl SgFilterVoConv {
    pub(crate) async fn ids_to_filter(client_name: &str, filters: Vec<String>) -> TardisResult<Option<Vec<SgRouteFilter>>> {
        Ok(if filters.is_empty() {
            Some(
                join_all(
                    PluginVoService::list(
                        client_name,
                        PluginQueryDto {
                            ids: Some(filters),
                            ..Default::default()
                        }
                        .to_instance()?,
                    )
                    .await?
                    .into_iter()
                    .map(|f| f.to_model(client_name))
                    .collect::<Vec<_>>(),
                )
                .await
                .into_iter()
                .collect::<TardisResult<Vec<_>>>()?,
            )
        } else {
            None
        })
    }
}

#[async_trait]
impl VoConv<SgRouteFilter, SgFilterVo> for SgFilterVo {
    async fn to_model(self, _client_name: &str) -> TardisResult<SgRouteFilter> {
        Ok(SgRouteFilter {
            code: self.code,
            name: self.name,
            spec: self.spec,
        })
    }

    async fn from_model(_model: SgRouteFilter) -> TardisResult<SgFilterVo> {
        todo!()
    }
}
