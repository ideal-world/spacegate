use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Mutex, OnceLock},
};

use crate::config::{
    matches_convert::{convert_config_to_kernel},
    plugin_filter_dto::FilterInstallExt,
    BackendHost, SgGateway, SgHttpRoute, SgProtocolConfig, SgRouteFilter, SgTlsMode,
};

use lazy_static::lazy_static;
use spacegate_kernel::{
    helper_layers::reload::Reloader,
    layers::gateway::{builder::default_gateway_route_fallback, create_http_router, SgGatewayRoute},
    listener::SgListen,
    service::get_http_backend_service,
    BoxError, BoxHyperService, Layer,
};
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use tardis::log::{instrument, warn};
use tardis::tokio::time::timeout;
use tardis::{config::config_dto::LogConfig, consts::IP_UNSPECIFIED};
use tardis::{
    log::{self as tracing, debug, info},
    log::{self, error},
    tokio::{self, sync::watch::Sender, task::JoinHandle},
    TardisFuns,
};
use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};
use tokio_util::sync::CancellationToken;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
    static ref START_JOIN_HANDLE: Arc<Mutex<HashMap<String, JoinHandle<()>>>> = <_>::default();
}

fn collect_tower_http_route(
    http_routes: Vec<crate::SgHttpRoute>,
    builder_ext: hyper::http::Extensions,
) -> Result<Vec<spacegate_kernel::layers::http_route::SgHttpRoute>, BoxError> {
    http_routes
        .into_iter()
        .map(|route| {
            let plugins = route.filters;
            let rules = route.rules;
            let rules = rules
                .into_iter()
                .map(|route_rule| {
                    let mut builder = spacegate_kernel::layers::http_route::SgHttpRouteRuleLayer::builder().ext(builder_ext.clone());
                    builder = if let Some(matches) = route_rule.matches {
                        builder.matches(matches.into_iter().map(convert_config_to_kernel).collect::<Result<Vec<_>, _>>()?)
                    } else {
                        builder.match_all()
                    };
                    let backends = route_rule
                        .backends
                        .into_iter()
                        .map(|backend| {
                            let host = backend.get_host();
                            let mut builder = spacegate_kernel::layers::http_route::SgHttpBackendLayer::builder().ext(builder_ext.clone());
                            let plugins = backend.filters;
                            #[cfg(feature = "k8s")]
                            {
                                use crate::extension::k8s_service::K8sService;
                                use spacegate_kernel::helper_layers::map_request::{add_extension::add_extension, MapRequestLayer};
                                use spacegate_kernel::SgBoxLayer;
                                if let BackendHost::K8sService(data) = backend.host {
                                    let namespace_ext = K8sService(data.into());
                                    builder = builder.plugin(SgBoxLayer::new(MapRequestLayer::new(add_extension(namespace_ext, true))))
                                }
                            }
                            builder = SgRouteFilter::install_on_backend(plugins, builder);
                            builder = builder.host(host).port(backend.port);
                            let protocol = backend.protocol;
                            if let Some(protocol) = protocol {
                                builder = builder.protocol(protocol.to_string());
                            }
                            builder.build()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    builder = builder.backends(backends);
                    if let Some(timeout) = route_rule.timeout_ms {
                        builder = builder.timeout(Duration::from_millis(timeout as u64));
                    }
                    let plugins = route_rule.filters;
                    builder = SgRouteFilter::install_on_rule(plugins, builder);
                    builder.build()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut builder = spacegate_kernel::layers::http_route::SgHttpRoute::builder().hostnames(route.hostnames.unwrap_or_default()).rules(rules).ext(builder_ext.clone());
            builder = SgRouteFilter::install_on_route(plugins, builder);
            builder.build()
        })
        .collect::<Result<Vec<_>, _>>()
}

/// Create a gateway service from plugins and http_routes
pub(crate) fn create_service(
    gateway_name: &str,
    cancel_token: CancellationToken,
    plugins: Vec<SgRouteFilter>,
    http_routes: Vec<crate::SgHttpRoute>,
    reloader: Reloader<SgGatewayRoute>,
    builder_ext: hyper::http::Extensions,
) -> Result<BoxHyperService, BoxError> {
    let routes = collect_tower_http_route(http_routes, builder_ext.clone())?;
    let builder = spacegate_kernel::layers::gateway::SgGatewayLayer::builder(gateway_name.to_owned(), cancel_token).http_routers(routes).http_route_reloader(reloader);

    let builder = SgRouteFilter::install_on_gateway(plugins, builder.ext(builder_ext));
    let gateway_layer = builder.build();
    let backend_service = get_http_backend_service();
    let service = BoxHyperService::new(gateway_layer.layer(backend_service));
    Ok(service)
}

/// create a new sg gateway route, which can be sent to reloader
pub(crate) fn create_router_service(http_routes: Vec<crate::SgHttpRoute>, builder_ext: hyper::http::Extensions) -> Result<SgGatewayRoute, BoxError> {
    let routes = collect_tower_http_route(http_routes, builder_ext)?;
    let service = create_http_router(&routes, default_gateway_route_fallback(), get_http_backend_service());
    Ok(service)
}

/// # Gateway
/// A running spacegate gateway instance
///
/// It's created by calling [start](RunningSgGateway::start).
///
/// And you can use [shutdown](RunningSgGateway::shutdown) to shutdown it manually.
///
/// Though, after it has been dropped, it will shutdown automatically.
pub struct RunningSgGateway {
    gateway_name: Arc<str>,
    token: CancellationToken,
    // _guard: tokio_util::sync::DropGuard,
    handle: tokio::task::JoinHandle<()>,
    pub reloader: Reloader<SgGatewayRoute>,
    shutdown_timeout: Duration,
}
impl std::fmt::Debug for RunningSgGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningSgGateway").field("shutdown_timeout", &self.shutdown_timeout).finish()
    }
}

pub static GLOBAL_STORE: OnceLock<Arc<Mutex<HashMap<String, RunningSgGateway>>>> = OnceLock::new();
impl RunningSgGateway {
    pub fn global_store() -> Arc<Mutex<HashMap<String, RunningSgGateway>>> {
        GLOBAL_STORE.get_or_init(Default::default).clone()
    }
    pub fn global_save(gateway_name: impl Into<String>, gateway: RunningSgGateway) {
        let global_store = Self::global_store();
        let mut global_store = global_store.lock().expect("poisoned lock");
        global_store.insert(gateway_name.into(), gateway);
    }

    pub fn global_remove(gateway_name: impl AsRef<str>) -> Option<RunningSgGateway> {
        let global_store = Self::global_store();
        let mut global_store = global_store.lock().expect("poisoned lock");
        global_store.remove(gateway_name.as_ref())
    }

    pub async fn global_update(gateway_name: impl AsRef<str>, http_routes: Vec<crate::SgHttpRoute>) -> Result<(), BoxError> {
        let gateway_name = gateway_name.as_ref();
        let service = create_router_service(http_routes, Default::default())?;
        let reloader = {
            let store = Self::global_store();
            let global_store = store.lock().expect("poisoned lock");
            if let Some(gw) = global_store.get(gateway_name) {
                gw.reloader.clone()
            } else {
                warn!("no such gateway in global repository: {gateway_name}");
                return Ok(());
            }
        };
        reloader.reload(service).await;
        Ok(())
    }
    /// Start a gateway from plugins and http_routes
    #[instrument(fields(gateway=%config.name), skip_all, err)]
    pub fn create(config: SgGateway, http_routes: Vec<SgHttpRoute>, cancel_token: CancellationToken) -> Result<Self, BoxError> {
        let mut builder_ext = hyper::http::Extensions::new();
        #[cfg(feature = "cache")]
        {
            if let Some(url) = &config.parameters.redis_url {
                let url: Arc<str> = url.clone().into();
                let name = config.name.clone();

                builder_ext.insert(crate::extension::redis_url::RedisUrl(url.clone()));
                tokio::spawn(async move {
                    // Initialize cache instances
                    log::trace!("Initialize cache client...url:{url}");
                    match crate::cache_client::init(name, &url).await {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("Initialize cache client failed:{e}");
                        }
                    }
                });
            }
        }
        log::info!("[SG.Server] start gateway");
        let reloader = <Reloader<SgGatewayRoute>>::default();
        let service = create_service(&config.name, cancel_token.clone(), config.filters, http_routes, reloader.clone(), builder_ext)?;
        if config.listeners.is_empty() {
            return Err("[SG.Server] Missing Listeners".into());
        }
        if let Some(log_level) = config.parameters.log_level.clone() {
            log::debug!("[SG.Server] change log level to {log_level}");
            let fw_config = TardisFuns::fw_config();
            let old_configs = fw_config.log();
            let directive = format!("{domain}={log_level}", domain = crate::constants::DOMAIN_CODE).parse().expect("invalid directive");
            let mut directives = old_configs.directives.clone();
            if let Some(index) = directives.iter().position(|d| d.to_string().starts_with(crate::constants::DOMAIN_CODE)) {
                directives.remove(index);
            }
            directives.push(directive);
            TardisFuns::tracing().update_config(&LogConfig {
                level: old_configs.level.clone(),
                directives,
                ..Default::default()
            })?;
        }

        let gateway_name: Arc<str> = Arc::from(config.name.to_string());
        let mut listens: Vec<SgListen<BoxHyperService>> = Vec::new();
        for listener in &config.listeners {
            let ip = listener.ip.unwrap_or(IP_UNSPECIFIED);
            let addr = SocketAddr::new(ip, listener.port);

            let gateway_name = gateway_name.clone();
            let protocol = listener.protocol.to_string();
            let mut tls_cfg = None;
            if let SgProtocolConfig::Https { ref tls } = listener.protocol {
                log::debug!("[SG.Server] Tls is init...mode:{:?}", tls.mode);
                if SgTlsMode::Terminate == tls.mode {
                    {
                        let certs = rustls_pemfile::certs(&mut tls.cert.as_bytes()).filter_map(Result::ok).collect::<Vec<_>>();
                        let mut tls_key = tls.key.as_bytes();
                        let mut keys = rustls_pemfile::read_all(&mut tls_key).filter_map(Result::ok);

                        let key = keys.find_map(|key| {
                            debug!("key item: {:?}", key);
                            match key {
                                rustls_pemfile::Item::Pkcs1Key(k) => Some(PrivateKeyDer::Pkcs1(k)),
                                rustls_pemfile::Item::Pkcs8Key(k) => Some(PrivateKeyDer::Pkcs8(k)),
                                rustls_pemfile::Item::Sec1Key(k) => Some(PrivateKeyDer::Sec1(k)),
                                rest => {
                                    warn!("Unsupported key type: {:?}", rest);
                                    None
                                }
                            }
                        });
                        if let Some(key) = key {
                            info!("[SG.Server] using cert key {key:?}");
                            let mut tls_server_cfg = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;
                            tls_server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                            tls_cfg.replace(tls_server_cfg);
                        } else {
                            error!("[SG.Server] Can not found a valid Tls private key");
                        }
                    };
                }
            }
            let listen_id = format!("{gateway_name}-{name}-{protocol}", name = listener.name, protocol = protocol);
            let mut listen = SgListen::new(addr, service.clone(), cancel_token.child_token(), listen_id);
            if let Some(tls_cfg) = tls_cfg {
                listen = listen.with_tls_config(tls_cfg);
            }
            listens.push(listen)
        }

        let local_set = tokio::task::LocalSet::new();
        for listen in listens {
            local_set.spawn_local(async move {
                let id = listen.listener_id.clone();
                if let Err(e) = listen.listen().await {
                    log::error!("[Sg.Server] listen error: {e}")
                }
                log::info!("[Sg.Server] listener[{id}] quit listening")
            });
        }

        // let cancel_guard = cancel_token.clone().drop_guard();
        let cancel_task = cancel_token.clone().cancelled_owned();
        let handle = {
            let gateway_name = gateway_name.clone();
            tokio::task::spawn_local(async move {
                log::info!(gateway = gateway_name.as_ref(), "[Sg.Server] start all listeners");
                local_set.run_until(cancel_task).await;
                log::info!(gateway = gateway_name.as_ref(), "[Sg.Server] cancelled");
            })
        };
        log::info!("[SG.Server] start finished");
        Ok(RunningSgGateway {
            gateway_name: gateway_name.clone(),
            token: cancel_token,
            // _guard: cancel_guard,
            handle,
            shutdown_timeout: Duration::from_secs(10),
            reloader,
        })
    }

    /// Shutdown this gateway
    pub async fn shutdown(self) {
        self.token.cancel();
        #[cfg(feature = "cache")]
        {
            let name = self.gateway_name.clone();
            log::trace!("[SG.Cache] Remove cache client...");
            tokio::spawn(async move { crate::cache_client::remove(name.as_ref()).await });
        }
        match timeout(self.shutdown_timeout, self.handle).await {
            Ok(_) => {}
            Err(e) => {
                log::warn!("[SG.Server] Wait shutdown timeout:{e}");
            }
        };
        log::info!("[SG.Server] Gateway shutdown");
    }
}
