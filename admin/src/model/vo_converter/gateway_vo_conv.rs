use crate::model::query_dto::PluginQueryDto;
use crate::model::vo::gateway_vo::{SgGatewayVO, SgListenerVO, SgTlsConfigVO};
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::plugin_service::PluginVoService;
use crate::service::secret_service::TlsConfigVoService;
use kernel_common::inner_model::gateway::{SgGateway, SgListener, SgTls, SgTlsConfig};
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

#[async_trait]
impl VoConv<SgGateway, SgGatewayVO> for SgGatewayVO {
    async fn to_model(self) -> TardisResult<SgGateway> {
        let filters = if let Some(filter_strs) = self.filters {
            Some(
                join_all(
                    PluginVoService::list(PluginQueryDto {
                        ids: Some(filter_strs),
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
        todo!()
    }
}

#[async_trait]
impl VoConv<SgListener, SgListenerVO> for SgListenerVO {
    async fn to_model(self) -> TardisResult<SgListener> {
        let tls = if let Some(tls) = self.tls {
            Some(SgTlsConfig {
                mode: tls.mode,
                tls: TlsConfigVoService::get_by_id(&tls.name).await?.to_model().await?,
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
        let tls = if let Some(tls) = model.tls {
            Some(SgTlsConfigVO {
                name: tls.tls.name,
                mode: tls.mode,
            })
        } else {
            None
        };
        Ok(SgListenerVO {
            name: model.name,
            ip: model.ip,
            port: model.port,
            protocol: model.protocol,
            tls,
            hostname: model.hostname,
        })
    }
}
