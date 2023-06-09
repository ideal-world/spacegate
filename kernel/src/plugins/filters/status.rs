use std::{mem::swap, sync::Arc};

use async_trait::async_trait;
use http::{header, HeaderValue};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    log,
    tokio::{
        self,
        sync::{watch::Sender, Mutex},
    },
    web::poem::{handler, listener::TcpListener, web::Html, Route, Server},
    TardisFuns,
};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext, SgRouteFilterRequestAction};
use crate::functions::http_route::SgHttpRouteMatchInst;
use lazy_static::lazy_static;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<Option<Sender<()>>>> = <_>::default();
}

pub mod status_plugin;

pub const CODE: &str = "status";
pub struct SgFilterStatusDef;

impl SgPluginFilterDef for SgFilterStatusDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterStatus>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterStatus {
    pub serv_addr: Option<String>,
    pub port: Option<u16>,
}

#[async_trait]
impl SgPluginFilter for SgFilterStatus {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    async fn init(&self) -> TardisResult<()> {
        let server_add = &self.serv_addr.clone().unwrap_or("0.0.0.0".to_string());
        let port = &self.port.clone().unwrap_or(8110);
        let addr = format!("{server_add}:{port}");
        let app = Route::new().at("/", status_plugin::create_status_html);
        let (shutdown_tx, _) = tokio::sync::watch::channel(());
        let mut shutdown_rx = shutdown_tx.subscribe();
         Server::new(TcpListener::bind("127.0.0.1:3000"))
            .run_with_graceful_shutdown(
                app,
                async move {
                    shutdown_rx.changed().await.ok();
                },
                None,
            )
            .await;
        let mut shutdown = SHUTDOWN_TX.lock().await;
        *shutdown = Some(shutdown_tx);
        log::info!("[SG.Filter.Status] Server started: {addr}");
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        let mut shutdown = SHUTDOWN_TX.lock().await;
        let mut swap_shutdown: Option<Sender<()>> = None;
        swap(&mut *shutdown, &mut swap_shutdown);
        if let Some(shutdown) = swap_shutdown {
            shutdown.send(()).ok();
            log::info!("[SG.Filter.Status] Server stopped");
        };
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRouteFilterContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use tardis::tokio;

    #[tokio::test]
    async fn test_status() {}
}
