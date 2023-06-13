use std::{mem::swap, sync::Arc};

use async_trait::async_trait;

use poem::{get, middleware::AddData, EndpointExt};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::result::TardisResult,
    log,
    tokio::{
        self,
        sync::{watch::Sender, Mutex},
    },
    web::poem::{listener::TcpListener, Route, Server},
    TardisFuns,
};

use self::status_plugin::update_status;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};
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
    pub title: Option<String>,
}

#[async_trait]
impl SgPluginFilter for SgFilterStatus {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    fn accept_error_response(&self) -> bool {
        true
    }

    async fn init(&self) -> TardisResult<()> {
        let server_add = &self.serv_addr.clone().unwrap_or("0.0.0.0".to_string());
        let port = &self.port.unwrap_or(8110);
        let title = self.title.clone().unwrap_or("System Status".to_string());
        let addr = format!("{server_add}:{port}");
        let app = Route::new().at("/", get(status_plugin::create_status_html.data(title)));
        let (shutdown_tx, _) = tokio::sync::watch::channel(());
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            log::info!("[SG.Filter.Status] Server started: {addr}");
            let _ = Server::new(TcpListener::bind(addr))
                .run_with_graceful_shutdown(
                    app,
                    async move {
                        shutdown_rx.changed().await.ok();
                    },
                    None,
                )
                .await;
        });

        let mut shutdown = SHUTDOWN_TX.lock().await;
        *shutdown = Some(shutdown_tx);
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

    async fn req_filter(&self, _: &str, ctx: SgRouteFilterContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRouteFilterContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRouteFilterContext)> {
        // if ctx.is_resp_error() {
        //     update_status(server_name, status)
        // }
        // else {
        //     update_status("test1", Status::Good).await;
        // }
        Ok((true, ctx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use tardis::tokio;

    use crate::plugins::filters::{
        status::{
            status_plugin::{update_status, Status},
            SgFilterStatus,
        },
        SgPluginFilter,
    };

    #[tokio::test]
    async fn test_status() {
        let stats = SgFilterStatus {
            serv_addr: Some("0.0.0.0".to_string()),
            port: Some(8110),
            title: Some("System Status".to_string()),
        };
        stats.init().await.unwrap();
        update_status("test1", Status::Minor).await;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        update_status("test1", Status::Good).await;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        println!("{:?}", stats);
    }
}
