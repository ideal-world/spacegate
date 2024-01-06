use std::net::IpAddr;
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use http::Request;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Server};

use serde::{Deserialize, Serialize};
use tardis::chrono::{Duration, Utc};
use tardis::tokio::task::JoinHandle;
use tardis::{
    basic::result::TardisResult,
    log,
    tokio::{
        self,
        sync::{watch::Sender, Mutex},
    },
};

#[cfg(feature = "cache")]
use crate::functions::cache_client;

use self::status_plugin::{clean_status, get_status, update_status, Status};
use super::{SgAttachedLevel, SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};
use crate::def_filter;
use crate::plugins::filters::status::sliding_window::SlidingWindowCounter;
use lazy_static::lazy_static;
use tardis::basic::error::TardisError;
#[cfg(not(feature = "cache"))]
use tardis::tokio::sync::RwLock;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<u16, (Sender<()>, JoinHandle<Result<(), hyper::Error>>)>>> = Default::default();
}

pub mod sliding_window;
pub mod status_plugin;

def_filter!("status", SgFilterStatusDef, SgFilterStatus);

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SgFilterStatus {
    pub serv_addr: String,
    pub port: u16,
    pub title: String,
    /// Unhealthy threshold , if server error more than this, server will be tag as unhealthy
    pub unhealthy_threshold: u16,
    /// second
    pub interval: u64,
    #[cfg(not(feature = "cache"))]
    #[serde(skip)]
    counter: RwLock<SlidingWindowCounter>,
    #[cfg(feature = "cache")]
    pub status_cache_key: String,
    #[cfg(feature = "cache")]
    pub window_cache_key: String,
}

impl Default for SgFilterStatus {
    fn default() -> Self {
        Self {
            serv_addr: "0.0.0.0".to_string(),
            port: 8110,
            title: "System Status".to_string(),
            unhealthy_threshold: 3,
            interval: 5,
            #[cfg(feature = "cache")]
            status_cache_key: "spacegate:cache:plugin:status".to_string(),
            #[cfg(feature = "cache")]
            window_cache_key: sliding_window::DEFAULT_CONF_WINDOW_KEY.to_string(),
            #[cfg(not(feature = "cache"))]
            counter: RwLock::new(SlidingWindowCounter::new(Duration::seconds(3), 60)),
        }
    }
}

#[async_trait]
impl SgPluginFilter for SgFilterStatus {
    fn accept(&self) -> super::SgPluginFilterAccept {
        super::SgPluginFilterAccept {
            kind: vec![super::SgPluginFilterKind::Http],
            accept_error_response: true,
        }
    }

    async fn init(&mut self, init_dto: &SgPluginFilterInitDto) -> TardisResult<()> {
        if !init_dto.attached_level.eq(&SgAttachedLevel::Gateway) {
            log::error!("[SG.Filter.Status] init filter is only can attached to gateway");
            return Ok(());
        }
        let (shutdown_tx, _) = tokio::sync::watch::channel(());
        let mut shutdown_rx = shutdown_tx.subscribe();

        let mut shutdown = SHUTDOWN_TX.lock().await;
        if let Some(old_shutdown) = shutdown.remove(&self.port) {
            old_shutdown.0.send(()).ok();
            let _ = old_shutdown.1.await;
            log::trace!("[SG.Filter.Status] init stop old service.");
        }

        let addr_ip: IpAddr = self.serv_addr.parse().map_err(|e| TardisError::conflict(&format!("[SG.Filter.Status] serv_addr parse error: {e}"), ""))?;
        let addr = (addr_ip, self.port).into();
        let title = Arc::new(Mutex::new(self.title.clone()));
        let gateway_name = Arc::new(Mutex::new(init_dto.gateway_name.clone()));
        let cache_key = Arc::new(Mutex::new(get_cache_key(self, &init_dto.gateway_name)));
        let make_svc = make_service_fn(move |_conn| {
            let title = title.clone();
            let gateway_name = gateway_name.clone();
            let cache_key = cache_key.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |request: Request<Body>| {
                    status_plugin::create_status_html(request, gateway_name.clone(), cache_key.clone(), title.clone())
                }))
            }
        });

        let server = match Server::try_bind(&addr) {
            Ok(server) => server.serve(make_svc),
            Err(e) => return Err(TardisError::conflict(&format!("[SG.Filter.Status] bind error: {e}"), "")),
        };

        let join = tokio::spawn(async move {
            log::info!("[SG.Filter.Status] Server started: {addr}");
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            server.await
        });
        (*shutdown).insert(self.port, (shutdown_tx, join));

        #[cfg(feature = "cache")]
        {
            clean_status(&get_cache_key(self, &init_dto.gateway_name), &init_dto.gateway_name).await?;
        }
        #[cfg(not(feature = "cache"))]
        {
            clean_status().await?;
        }
        for http_route_rule in init_dto.http_route_rules.clone() {
            if let Some(backends) = &http_route_rule.backends {
                for backend in backends {
                    #[cfg(feature = "cache")]
                    {
                        let cache_client = cache_client::get(&init_dto.gateway_name).await?;
                        update_status(
                            &backend.name_or_host,
                            &get_cache_key(self, &init_dto.gateway_name),
                            &cache_client,
                            status_plugin::Status::default(),
                        )
                        .await?;
                    }
                    #[cfg(not(feature = "cache"))]
                    {
                        update_status(&backend.name_or_host, status_plugin::Status::default()).await?;
                    }
                }
            }
        }
        #[cfg(not(feature = "cache"))]
        {
            self.counter = RwLock::new(SlidingWindowCounter::new(Duration::seconds(self.interval as i64), 60));
        }
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        let mut shutdown = SHUTDOWN_TX.lock().await;

        if let Some(shutdown) = shutdown.remove(&self.port) {
            shutdown.0.send(()).ok();
            let _ = shutdown.1.await;
            log::info!("[SG.Filter.Status] Server stopped");
        };
        Ok(())
    }

    async fn req_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if let Some(backend_name) = ctx.get_chose_backend_name() {
            if ctx.is_resp_error() {
                let now = Utc::now();
                let count;
                #[cfg(not(feature = "cache"))]
                {
                    let mut counter = self.counter.write().await;
                    count = counter.add_and_count(now)
                }
                #[cfg(feature = "cache")]
                {
                    count = SlidingWindowCounter::new(Duration::seconds(self.interval as i64), &self.window_cache_key).add_and_count(now, &ctx).await?;
                }
                if count >= self.unhealthy_threshold as u64 {
                    #[cfg(feature = "cache")]
                    {
                        update_status(
                            &backend_name,
                            &get_cache_key(self, &ctx.get_gateway_name()),
                            &ctx.cache().await?,
                            status_plugin::Status::Major,
                        )
                        .await?;
                    }
                    #[cfg(not(feature = "cache"))]
                    {
                        update_status(&backend_name, status_plugin::Status::Major).await?;
                    }
                } else {
                    #[cfg(feature = "cache")]
                    {
                        update_status(
                            &backend_name,
                            &get_cache_key(self, &ctx.get_gateway_name()),
                            &ctx.cache().await?,
                            status_plugin::Status::Minor,
                        )
                        .await?;
                    }
                    #[cfg(not(feature = "cache"))]
                    {
                        update_status(&backend_name, status_plugin::Status::Minor).await?;
                    }
                }
            } else {
                let gotten_status: Option<Status>;
                #[cfg(feature = "cache")]
                {
                    gotten_status = get_status(&backend_name, &get_cache_key(self, &ctx.get_gateway_name()), &ctx.cache().await?).await?;
                }
                #[cfg(not(feature = "cache"))]
                {
                    gotten_status = get_status(&backend_name).await?;
                }
                if let Some(status) = gotten_status {
                    if status != status_plugin::Status::Good {
                        #[cfg(feature = "cache")]
                        {
                            update_status(
                                &backend_name,
                                &get_cache_key(self, &ctx.get_gateway_name()),
                                &ctx.cache().await?,
                                status_plugin::Status::Good,
                            )
                            .await?;
                        }
                        #[cfg(not(feature = "cache"))]
                        {
                            update_status(&backend_name, status_plugin::Status::Good).await?;
                        }
                    }
                }
            }
        }
        Ok((true, ctx))
    }
}

#[cfg(feature = "cache")]
fn get_cache_key(filter_status: &SgFilterStatus, gateway_name: &str) -> String {
    format!("{}:{}", filter_status.status_cache_key, gateway_name)
}
#[cfg(not(feature = "cache"))]
// not use in not cache mode;
fn get_cache_key(_: &SgFilterStatus, _: &str) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use std::env;

    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use hyper::Body;
    use tardis::{
        basic::{error::TardisError, result::TardisResult},
        test::test_container::TardisTestContainer,
        testcontainers::{self, clients::Cli, Container},
        tokio,
    };
    use testcontainers_modules::redis::Redis;

    #[cfg(feature = "cache")]
    use crate::functions;
    #[cfg(feature = "cache")]
    use crate::plugins::filters::status::get_cache_key;
    use crate::{
        config::{
            gateway_dto::SgParameters,
            http_route_dto::{SgBackendRef, SgHttpRouteRule},
        },
        instance::{SgBackendInst, SgHttpRouteRuleInst},
        plugins::{
            context::ChosenHttpRouteRuleInst,
            filters::{
                status::{
                    status_plugin::{get_status, Status},
                    SgFilterStatus,
                },
                SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext,
            },
        },
    };

    #[tokio::test]
    async fn test_status() {
        env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
        tracing_subscriber::fmt::init();
        let mut stats = SgFilterStatus::default();
        let mock_backend_ref = SgBackendRef {
            name_or_host: "test1".to_string(),
            namespace: None,
            port: 80,
            timeout_ms: None,
            protocol: Some(crate::config::gateway_dto::SgProtocol::Http),
            weight: None,
            filters: None,
        };
        let docker = testcontainers::clients::Cli::default();
        let _x = docker_init(&docker).await.unwrap();
        let gateway_name = "gateway_name1".to_string();

        #[cfg(feature = "cache")]
        functions::cache_client::init(&gateway_name, &env::var("TARDIS_FW.CACHE.URL").unwrap()).await.unwrap();

        stats
            .init(&SgPluginFilterInitDto {
                gateway_name: gateway_name.clone(),
                gateway_parameters: SgParameters::default(),
                http_route_rules: vec![SgHttpRouteRule {
                    matches: None,
                    filters: None,
                    backends: Some(vec![mock_backend_ref.clone()]),
                    timeout_ms: None,
                }],
                attached_level: crate::plugins::filters::SgAttachedLevel::Gateway,
            })
            .await
            .unwrap();
        let mock_backend = SgBackendInst {
            name_or_host: mock_backend_ref.name_or_host,
            namespace: mock_backend_ref.namespace,
            port: mock_backend_ref.port,
            timeout_ms: mock_backend_ref.timeout_ms,
            protocol: mock_backend_ref.protocol,
            weight: mock_backend_ref.weight,
            filters: vec![],
        };
        let mut ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group/iam/ct/001?name=sg"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "127.0.0.1:8080".parse().unwrap(),
            gateway_name.clone(),
            Some(ChosenHttpRouteRuleInst::cloned_from(&SgHttpRouteRuleInst { ..Default::default() }, None)),
            None,
        );

        ctx.set_chose_backend_inst(&mock_backend);

        let ctx = ctx.resp_from_error(TardisError::bad_request("mock resp error", ""));
        let (is_ok, ctx) = stats.resp_filter("id1", ctx).await.unwrap();
        assert!(is_ok);

        let gotten_status: Status;
        #[cfg(feature = "cache")]
        {
            gotten_status = get_status(&mock_backend.name_or_host, &get_cache_key(&stats, &ctx.get_gateway_name()), ctx.cache().await.unwrap()).await.unwrap().unwrap();
        }
        #[cfg(not(feature = "cache"))]
        {
            gotten_status = get_status(&mock_backend.name_or_host).await.unwrap().unwrap();
        }
        assert_eq!(gotten_status, Status::Minor);

        let (_, ctx) = stats.resp_filter("id2", ctx).await.unwrap();
        let (_, ctx) = stats.resp_filter("id3", ctx).await.unwrap();
        let (_, ctx) = stats.resp_filter("id4", ctx).await.unwrap();

        let gotten_status: Status;
        #[cfg(feature = "cache")]
        {
            gotten_status = get_status(&mock_backend.name_or_host, &get_cache_key(&stats, &ctx.get_gateway_name()), ctx.cache().await.unwrap()).await.unwrap().unwrap();
        }
        #[cfg(not(feature = "cache"))]
        {
            gotten_status = get_status(&mock_backend.name_or_host).await.unwrap().unwrap();
        }
        assert_eq!(gotten_status, Status::Major);

        let ctx = ctx.resp(StatusCode::OK, HeaderMap::new(), Body::empty());
        let (_, _ctx) = stats.resp_filter("id4", ctx).await.unwrap();

        let gotten_status: Status;
        #[cfg(feature = "cache")]
        {
            gotten_status = get_status(&mock_backend.name_or_host, &get_cache_key(&stats, &_ctx.get_gateway_name()), _ctx.cache().await.unwrap()).await.unwrap().unwrap();
        }
        #[cfg(not(feature = "cache"))]
        {
            gotten_status = get_status(&mock_backend.name_or_host).await.unwrap().unwrap();
        }
        assert_eq!(gotten_status, Status::Good);
    }

    pub struct LifeHold<'a> {
        pub redis: Container<'a, Redis>,
    }

    async fn docker_init(docker: &Cli) -> TardisResult<LifeHold<'_>> {
        let redis_container = TardisTestContainer::redis_custom(docker);
        let port = redis_container.get_host_port_ipv4(6379);
        let url = format!("redis://127.0.0.1:{port}/0",);
        env::set_var("TARDIS_FW.CACHE.URL", url);

        Ok(LifeHold { redis: redis_container })
    }
}
