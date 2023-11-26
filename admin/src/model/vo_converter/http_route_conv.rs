use crate::model::query_dto::{BackendRefQueryDto, ToInstance};
use crate::model::vo::backend_vo::SgBackendRefVo;
use crate::model::vo::http_route_vo::{SgHttpRouteRuleVo, SgHttpRouteVo};
use crate::model::vo_converter::plugin_vo_conv::SgFilterVoConv;
use crate::model::vo_converter::VoConv;
use crate::service::backend_ref_service::BackendRefVoService;
use kernel_common::inner_model::http_route::{SgBackendRef, SgHttpRoute, SgHttpRouteRule};
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

#[async_trait]
impl VoConv<SgHttpRoute, SgHttpRouteVo> for SgHttpRouteVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgHttpRoute> {
        Ok(SgHttpRoute {
            name: self.name,
            gateway_name: self.gateway_name,
            hostnames: self.hostnames,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters).await?,
            rules: if self.rules.is_empty() {
                Some(join_all(self.rules.into_iter().map(|r| r.to_model(client_name)).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?)
            } else {
                None
            },
        })
    }

    async fn from_model(_model: SgHttpRoute) -> TardisResult<SgHttpRouteVo> {
        todo!()
    }
}

struct SgBackendRefVoConv;

impl SgBackendRefVoConv {
    async fn ids_to_backends(client_name: &str, ids: Vec<String>) -> TardisResult<Option<Vec<SgBackendRef>>> {
        Ok(if ids.is_empty() {
            Some(
                join_all(
                    BackendRefVoService::list(
                        client_name,
                        BackendRefQueryDto {
                            names: Some(ids),
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
impl VoConv<SgHttpRouteRule, SgHttpRouteRuleVo> for SgHttpRouteRuleVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgHttpRouteRule> {
        Ok(SgHttpRouteRule {
            matches: self.matches,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters).await?,
            backends: SgBackendRefVoConv::ids_to_backends(client_name, self.backends).await?,
            timeout_ms: self.timeout_ms,
        })
    }

    async fn from_model(_model: SgHttpRouteRule) -> TardisResult<SgHttpRouteRuleVo> {
        todo!()
    }
}

#[async_trait]
impl VoConv<SgBackendRef, SgBackendRefVo> for SgBackendRefVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgBackendRef> {
        Ok(SgBackendRef {
            name_or_host: self.name_or_host,
            namespace: self.namespace,
            port: self.port,
            timeout_ms: self.timeout_ms,
            protocol: self.protocol,
            weight: self.weight,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters.unwrap_or_default()).await?,
        })
    }

    async fn from_model(_model: SgBackendRef) -> TardisResult<SgBackendRefVo> {
        todo!()
    }
}
