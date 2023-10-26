use crate::model::query_dto::PluginQueryDto;
use crate::model::vo::gateway_vo::{SgGatewayVO, SgListenerVO, SgTlsConfigVO};
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::plugin_service::PluginServiceVo;
use crate::service::tls_config_service::TlsConfigServiceVo;
use kernel_common::inner_model::gateway::{SgGateway, SgListener, SgTlsConfig};
use kernel_common::inner_model::plugin_filter::SgRouteFilter;
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;
use tardis::TardisFuns;

#[async_trait]
impl VoConv<SgGateway, SgGatewayVO> for SgGatewayVO {
    async fn to_model(self) -> TardisResult<SgGateway> {
        let filters = if let Some(filter_strs) = self.filters {
            Some(
                join_all(
                    PluginServiceVo::list(PluginQueryDto {
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
        let tls = if let Some(tls_id) = self.tls {
            Some(TlsConfigServiceVo::get_by_id(&tls_id).await?.to_model().await?)
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

    async fn from_model(model: SgListener) -> TardisResult<SgListenerVO> {
        todo!()
    }
}

#[async_trait]
impl VoConv<SgTlsConfig, SgTlsConfigVO> for SgTlsConfigVO {
    async fn to_model(self) -> TardisResult<SgTlsConfig> {
        Ok(SgTlsConfig {
            mode: self.mode,
            key: self.key,
            cert: self.cert,
        })
    }

    async fn from_model(model: SgTlsConfig) -> TardisResult<SgTlsConfigVO> {
        Ok(SgTlsConfigVO {
            id: TardisFuns::crypto.digest.md5(&format!("{}{}{}", model.mode.clone().to_kube_tls_mode_type().to_string(), model.key, model.cert))?,
            mode: model.mode,
            key: model.key,
            cert: model.cert,
            ref_ids: None,
        })
    }
}
