use crate::model::query_dto::PluginQueryDto;
use crate::model::vo::gateway_vo::{SgGatewayVO, SgListenerVO, SgTlsConfigVO};
use crate::model::vo::plugin_vo::SgFilterVO;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::plugin_service::PluginVoService;
use crate::service::secret_service::TlsConfigVoService;
use kernel_common::helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique};
use kernel_common::inner_model::gateway::{SgGateway, SgListener, SgTls, SgTlsConfig};
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

#[async_trait]
impl VoConv<SgGateway, SgGatewayVO> for SgGatewayVO {
    async fn to_model(self) -> TardisResult<SgGateway> {
        let filters = if !self.filters.is_empty() {
            Some(
                join_all(
                    PluginVoService::list(PluginQueryDto {
                        ids: Some(self.filters),
                        ..Default::default()
                    })
                    .await?
                    .into_iter()
                    .map(|f| f.to_model())
                    .collect::<Vec<_>>(),
                )
                .await
                .into_iter()
                .collect::<TardisResult<Vec<_>>>()?,
            )
        } else {
            None
        };
        Ok(SgGateway {
            name: self.name,
            parameters: self.parameters,
            listeners: join_all(self.listeners.into_iter().map(|l| l.to_model()).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?,
            filters,
        })
    }

    async fn from_model(model: SgGateway) -> TardisResult<SgGatewayVO> {
        let (namespace, _) = parse_k8s_obj_unique(&model.name);
        let listeners = join_all(model.listeners.into_iter().map(|l| SgListenerVO::from_model(l)).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?;
        let tls = listeners.iter().map(|l| l.tls_vo.clone()).filter(|t| t.is_some()).map(|t| t.unwrap()).collect::<Vec<_>>();
        let filter_vo = if let Some(filters) = model.filters {
            filters
                .into_iter()
                .map(|f| SgFilterVO {
                    id: format_k8s_obj_unique(Some(&namespace), &f.code),
                    code: f.code,
                    name: f.name,
                    spec: f.spec,
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let filters = filter_vo.iter().map(|f| f.id.clone()).collect();

        Ok(SgGatewayVO {
            name: model.name,
            parameters: model.parameters,
            listeners,
            filters,
            tls,
            filter_vos: filter_vo,
        })
    }
}

#[async_trait]
impl VoConv<SgListener, SgListenerVO> for SgListenerVO {
    async fn to_model(self) -> TardisResult<SgListener> {
        let tls = if let Some(tls) = self.tls {
            Some(SgTlsConfig {
                mode: tls.mode,
                tls: TlsConfigVoService::get_by_id(&tls.name).await?,
            })
        } else {
            None
        };

        Ok(SgListener {
            name: self.name,
            ip: self.ip,
            port: self.port,
            protocol: self.protocol,
            tls,
            hostname: self.hostname,
        })
    }

    async fn from_model(model: SgListener) -> TardisResult<Self> {
        Ok(SgListenerVO {
            name: model.name,
            ip: model.ip,
            port: model.port,
            protocol: model.protocol,
            tls: model.tls.clone().map(|t| SgTlsConfigVO { name: t.tls.name, mode: t.mode }),
            hostname: model.hostname,
            tls_vo: model.tls.map(|t| SgTls {
                name: t.tls.name,
                key: t.tls.key,
                cert: t.tls.cert,
            }),
        })
    }
}
