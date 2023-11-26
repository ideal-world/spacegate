use crate::model::vo::gateway_vo::{SgGatewayVo, SgListenerVo, SgTlsConfigVo};
use crate::model::vo::plugin_vo::SgFilterVo;
use crate::model::vo_converter::plugin_vo_conv::SgFilterVoConv;
use crate::model::vo_converter::VoConv;
use crate::service::base_service::VoBaseService;
use crate::service::secret_service::TlsVoService;
use kernel_common::inner_model::gateway::{SgGateway, SgListener, SgTls, SgTlsConfig};
use tardis::async_trait::async_trait;
use tardis::basic::result::TardisResult;
use tardis::futures_util::future::join_all;

#[async_trait]
impl VoConv<SgGateway, SgGatewayVo> for SgGatewayVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgGateway> {
        Ok(SgGateway {
            name: self.name,
            parameters: self.parameters,
            listeners: join_all(self.listeners.into_iter().map(|l| l.to_model(client_name)).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?,
            filters: SgFilterVoConv::ids_to_filter(client_name, self.filters).await?,
        })
    }

    async fn from_model(model: SgGateway) -> TardisResult<SgGatewayVo> {
        let listeners = join_all(model.listeners.into_iter().map(SgListenerVo::from_model).collect::<Vec<_>>()).await.into_iter().collect::<TardisResult<Vec<_>>>()?;
        let tls = listeners.iter().filter_map(|l| l.tls_vo.clone()).collect::<Vec<_>>();
        let filter_vo = if let Some(filters) = model.filters {
            filters
                .into_iter()
                .map(|f| SgFilterVo {
                    id: format!("{}{}", &model.name, &f.code),
                    code: f.code,
                    name: f.name,
                    spec: f.spec,
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let filters = filter_vo.iter().map(|f| f.id.clone()).collect();

        Ok(SgGatewayVo {
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
impl VoConv<SgListener, SgListenerVo> for SgListenerVo {
    async fn to_model(self, client_name: &str) -> TardisResult<SgListener> {
        let tls = if let Some(tls) = self.tls {
            Some(SgTlsConfig {
                mode: tls.mode,
                tls: TlsVoService::get_by_id(client_name, &tls.name).await?,
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
        Ok(SgListenerVo {
            name: model.name,
            ip: model.ip,
            port: model.port,
            protocol: model.protocol,
            tls: model.tls.clone().map(|t| SgTlsConfigVo { name: t.tls.name, mode: t.mode }),
            hostname: model.hostname,
            tls_vo: model.tls.map(|t| SgTls {
                name: t.tls.name,
                key: t.tls.key,
                cert: t.tls.cert,
            }),
        })
    }
}
