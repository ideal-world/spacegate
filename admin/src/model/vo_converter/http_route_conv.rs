use std::mem;

use crate::model::query_dto::{BackendRefQueryDto, ToInstance};
use crate::model::vo::backend_vo::SgBackendRefVo;
use crate::model::vo::http_route_vo::{SgHttpRouteRuleVo, SgHttpRouteVo};
use crate::model::vo::plugin_vo::SgFilterVo;
use crate::model::vo_converter::plugin_vo_conv::SgFilterVoConv;
use crate::model::vo_converter::VoConv;
use crate::service::backend_ref_service::BackendRefVoService;
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
use kernel_common::inner_model::http_route::{SgBackendRef, SgHttpRoute, SgHttpRouteRule};
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

#[async_trait]
impl VoConv<SgHttpRoute, SgHttpRouteVo> for SgHttpRouteVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgHttpRoute> {
        let result = SgHttpRoute {
            name: self.name,
            gateway_name: self.gateway_name,
            priority: self.priority,
            hostnames: self.hostnames,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters).await?,
            rules: if !self.rules.is_empty() {
                Some(SgHttpRouteRuleVo::to_vec_model(client_name, self.rules).await?)
            } else {
                None
            },
        };
        Ok(result)
    }

    async fn from_model(model: SgHttpRoute) -> TardisResult<SgHttpRouteVo> {
        let mut rules = SgHttpRouteRuleVo::from_vec_model(model.rules).await?;
        let backend_vos = rules.iter_mut().map(|r| mem::take(&mut r.backend_vos)).flatten().collect::<Vec<_>>();

        let mut filter_vos = SgFilterVo::from_vec_model(model.filters).await?;
        let filters = SgFilterVoConv::filters_to_ids(&filter_vos);
        filter_vos.append(&mut rules.iter_mut().map(|r| mem::take(&mut r.filter_vos)).flatten().collect());

        Ok(SgHttpRouteVo {
            name: model.name,
            gateway_name: model.gateway_name,
            hostnames: model.hostnames,
            priority: model.priority,
            filters,
            rules,
            filter_vos,
            backend_vos,
        })
    }
}

struct SgBackendRefVoConv;

impl SgBackendRefVoConv {
    async fn ids_to_backends(client_name: &str, ids: Vec<String>) -> TardisResult<Option<Vec<SgBackendRef>>> {
        Ok(if !ids.is_empty() {
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
        let result = SgHttpRouteRule {
            matches: self.matches,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters).await?,
            backends: SgBackendRefVoConv::ids_to_backends(client_name, self.backends).await?,
            timeout_ms: self.timeout_ms,
        };
        Ok(result)
    }

    async fn from_model(model: SgHttpRouteRule) -> TardisResult<SgHttpRouteRuleVo> {
        let mut filter_vos = SgFilterVo::from_vec_model(model.filters).await?;
        let filters = SgFilterVoConv::filters_to_ids(&filter_vos);
        let mut backend_vos = SgBackendRefVo::from_vec_model(model.backends).await?;
        filter_vos.append(&mut backend_vos.iter_mut().map(|b| mem::take(&mut b.filter_vos)).flatten().collect());

        Ok(SgHttpRouteRuleVo {
            matches: model.matches,
            filters,
            backends: backend_vos.iter().map(|b| b.id.clone()).collect(),
            timeout_ms: model.timeout_ms,
            filter_vos,
            backend_vos,
        })
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

    async fn from_model(model: SgBackendRef) -> TardisResult<SgBackendRefVo> {
        let filter_vos = SgFilterVo::from_vec_model(model.filters).await?;
        let filters = SgFilterVoConv::filters_to_ids(&filter_vos);

        Ok(SgBackendRefVo {
            id: format!(
                "{}-{}-{}-{}-{}",
                model.name_or_host,
                model.namespace.clone().unwrap_or(DEFAULT_NAMESPACE.to_string()),
                model.port,
                model.protocol.clone().unwrap_or_default(),
                model.weight.clone().unwrap_or_default(),
            ),
            name_or_host: model.name_or_host,
            namespace: model.namespace,
            port: model.port,
            timeout_ms: model.timeout_ms,
            protocol: model.protocol,
            weight: model.weight,
            filters: if filters.is_empty() { None } else { Some(filters) },
            filter_vos,
        })
    }
}
