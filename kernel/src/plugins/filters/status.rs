use std::{collections::HashMap, mem::swap, sync::Arc};

use async_trait::async_trait;

use k8s_openapi::chrono::Utc;
use poem::{get, EndpointExt};
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

use self::status_plugin::{get_status, update_status};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgRouteFilterContext};
use crate::{config::http_route_dto::SgHttpRouteRule, functions::http_route::SgHttpRouteMatchInst};
use lazy_static::lazy_static;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<Option<Sender<()>>>> = <_>::default();
    static ref SERVER_ERR: Arc<Mutex<HashMap<String, (u16, i64)>>> = <_>::default();
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgFilterStatus {
    pub serv_addr: String,
    pub port: u16,
    pub title: String,
    /// Unhealthy threshold , if server error more than this, server will be tag as unhealthy
    pub unhealth_threshold: u16,
    pub interval: u64,
}

impl Default for SgFilterStatus {
    fn default() -> Self {
        Self {
            serv_addr: "0.0.0.0".to_string(),
            port: 8110,
            title: "System Status".to_string(),
            unhealth_threshold: 3,
            interval: 5,
        }
    }
}

#[async_trait]
impl SgPluginFilter for SgFilterStatus {
    fn kind(&self) -> super::SgPluginFilterKind {
        super::SgPluginFilterKind::Http
    }

    fn accept_error_response(&self) -> bool {
        true
    }

    async fn init(&self, http_route_rules: &Vec<SgHttpRouteRule>) -> TardisResult<()> {
        let addr = format!("{}:{}", self.serv_addr, self.port);
        let app = Route::new().at("/", get(status_plugin::create_status_html.data(self.title.clone())));
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

        for http_route_rule in http_route_rules {
            if let Some(backends) = &http_route_rule.backends {
                for backend in backends {
                    update_status(&backend.name_or_host, status_plugin::Status::default()).await;
                }
            }
        }
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
        if let Some(backend_name) = ctx.get_backend_name() {
            if ctx.is_resp_error() {
                let mut server_err = SERVER_ERR.lock().await;
                if let Some((times, expire)) = server_err.get(&backend_name) {
                    let now = Utc::now().timestamp();
                    if *expire > now {
                        if *times > self.unhealth_threshold {
                            update_status(&backend_name, status_plugin::Status::Major).await;
                        } else {
                            let new_times = *times + 1;
                            server_err.insert(backend_name.clone(), (new_times, now + self.interval as i64));
                            update_status(&backend_name, status_plugin::Status::Minor).await;
                        }
                    } else {
                        server_err.insert(backend_name.clone(), (1, now + self.interval as i64));
                    }
                }
                update_status(&backend_name, status_plugin::Status::Major).await;
            } else if let Some(status) = get_status(&backend_name).await {
                if status != status_plugin::Status::Good {
                    update_status(&backend_name, status_plugin::Status::Good).await;
                }
            }
        }
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
        let stats = SgFilterStatus::default();
        stats.init(&vec![]).await.unwrap();
        update_status("test1", Status::Minor).await;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        update_status("test1", Status::Good).await;
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        println!("{:?}", stats);
    }
}
