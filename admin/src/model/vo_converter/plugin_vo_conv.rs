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
            None
        } else {
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
        })
    }

    pub(crate) fn filters_to_ids(filters: &[SgFilterVo]) -> Vec<String> {
        filters.iter().map(|f| f.id.clone()).collect()
    }
}

#[async_trait]
impl VoConv<SgRouteFilter, SgFilterVo> for SgFilterVo {
    async fn to_model(self, _client_name: &str) -> TardisResult<SgRouteFilter> {
        Ok(SgRouteFilter {
            code: self.code,
            name: self.name,
            enable: self.enable,
            spec: self.spec,
        })
    }

    async fn from_model(model: SgRouteFilter) -> TardisResult<SgFilterVo> {
        let name = model.name.clone();
        Ok(SgFilterVo {
            id: format!("{}{}", &model.code, if let Some(name) = name { format!("_{}", name) } else { "".to_string() }),
            code: model.code,
            name: model.name,
            enable: model.enable,
            spec: model.spec,
        })
    }
}
