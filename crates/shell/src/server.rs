use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{Mutex, OnceLock},
};

use crate::config::{matches_convert::convert_config_to_kernel, plugin_filter_dto::global_batch_mount_plugin, PluginConfig, SgProtocolConfig, SgTlsMode};

use spacegate_config::{BackendHost, Config, ConfigItem, PluginInstanceId};
use spacegate_kernel::{
    helper_layers::reload::Reloader,
    listener::SgListen,
    service::gateway::{builder::default_gateway_route_fallback, create_http_router, HttpRouterService},
    ArcHyperService, BoxError,
};
use spacegate_plugin::{mount::MountPointIndex, PluginRepository};
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};
use tokio_util::sync::CancellationToken;

fn collect_http_route(
    gateway_name: Arc<str>,
    http_routes: impl IntoIterator<Item = (String, crate::SgHttpRoute)>,
) -> Result<HashMap<String, spacegate_kernel::service::http_route::HttpRoute>, BoxError> {
    http_routes
        .into_iter()
        .map(|(name, route)| {
            let route_name: Arc<str> = name.clone().into();
            let mount_index = MountPointIndex::HttpRoute {
                gateway: gateway_name.clone(),
                route: route_name.clone(),
            };
            let plugins = route.plugins;
            let rules = route.rules;
            let rules = rules
                .into_iter()
                .enumerate()
                .map(|(rule_index, route_rule)| {
                    let mount_index = MountPointIndex::HttpRouteRule {
                        rule: rule_index,
                        gateway: gateway_name.clone(),
                        route: route_name.clone(),
                    };
                    let mut builder = spacegate_kernel::service::http_route::HttpRouteRule::builder();
                    builder = if let Some(matches) = route_rule.matches {
                        builder.matches(matches.into_iter().map(convert_config_to_kernel).collect::<Result<Vec<_>, _>>()?)
                    } else {
                        builder.match_all()
                    };
                    let backends = route_rule
                        .backends
                        .into_iter()
                        .enumerate()
                        .map(|(backend_index, backend)| {
                            let mount_index = MountPointIndex::HttpBackend {
                                backend: backend_index,
                                rule: rule_index,
                                gateway: gateway_name.clone(),
                                route: route_name.clone(),
                            };
                            let host = backend.get_host();
                            let mut builder = spacegate_kernel::service::http_route::HttpBackend::builder();
                            let plugins = backend.plugins;
                            #[cfg(feature = "k8s")]
                            {
                                use crate::extension::k8s_service::K8sService;
                                use spacegate_config::model::BackendHost;
                                use spacegate_kernel::helper_layers::map_request::{add_extension::add_extension, MapRequestLayer};
                                use spacegate_kernel::BoxLayer;
                                if let BackendHost::K8sService(data) = backend.host {
                                    let namespace_ext = K8sService(data.into());
                                    // need to add to front
                                    builder = builder.plugin(SgBoxLayer::new(MapRequestLayer::new(add_extension(namespace_ext, true))))
                                }
                            }
                            builder = builder.host(host).port(backend.port);
                            if let Some(timeout) = backend.timeout_ms.map(|timeout| Duration::from_millis(timeout as u64)) {
                                builder = builder.timeout(timeout)
                            }
                            let mut layer = if let BackendHost::File { path } = backend.host {
                                builder.file().path(path).build()
                            } else if let Some(protocol) = backend.protocol {
                                builder.schema(protocol.to_string()).build()
                            } else {
                                builder.build()
                            };
                            global_batch_mount_plugin(plugins, &mut layer, mount_index);
                            Result::<_, BoxError>::Ok(layer)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    builder = builder.backends(backends);
                    if let Some(timeout) = route_rule.timeout_ms {
                        builder = builder.timeout(Duration::from_millis(timeout as u64));
                    }
                    let mut layer = builder.build();
                    global_batch_mount_plugin(route_rule.plugins, &mut layer, mount_index);
                    Result::<_, BoxError>::Ok(layer)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut layer =
                spacegate_kernel::service::http_route::HttpRoute::builder().hostnames(route.hostnames.unwrap_or_default()).rules(rules).priority(route.priority).build();
            global_batch_mount_plugin(plugins, &mut layer, mount_index);
            Ok((name, layer))
        })
        .collect::<Result<HashMap<String, _>, _>>()
}

/// Create a gateway service from plugins and http_routes
pub(crate) fn create_service(
    gateway_name: &str,
    plugins: Vec<PluginInstanceId>,
    http_routes: BTreeMap<String, crate::SgHttpRoute>,
    reloader: Reloader<HttpRouterService>,
) -> Result<ArcHyperService, BoxError> {
    let gateway_name: Arc<str> = gateway_name.into();
    let routes = collect_http_route(gateway_name.clone(), http_routes)?;
    let mut layer = spacegate_kernel::service::gateway::Gateway::builder(gateway_name.clone()).http_routers(routes).http_route_reloader(reloader).build();
    global_batch_mount_plugin(plugins, &mut layer, MountPointIndex::Gateway { gateway: gateway_name });
    let service = ArcHyperService::new(layer.as_service());
    Ok(service)
}

/// create a new sg gateway route, which can be sent to reloader
pub(crate) fn create_router_service(gateway_name: Arc<str>, http_routes: BTreeMap<String, crate::SgHttpRoute>) -> Result<HttpRouterService, BoxError> {
    let routes = collect_http_route(gateway_name, http_routes.clone())?;
    let service = create_http_router(routes.values(), default_gateway_route_fallback());
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
    pub gateway_name: Arc<str>,
    token: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
    pub reloader: Reloader<HttpRouterService>,
    shutdown_timeout: Duration,
}
impl std::fmt::Debug for RunningSgGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningSgGateway").field("shutdown_timeout", &self.shutdown_timeout).finish()
    }
}

pub static GLOBAL_STORE: OnceLock<Arc<Mutex<HashMap<String, RunningSgGateway>>>> = OnceLock::new();
impl RunningSgGateway {
    pub async fn global_init(config: Config, signal: CancellationToken) {
        for (id, spec) in config.plugins.into_inner() {
            if let Err(err) = PluginRepository::global().create_or_update_instance(PluginConfig { id: id.clone(), spec }) {
                tracing::error!("[SG.Config] fail to init plugin [{id}]: {err}", id = id.to_string());
            }
        }
        for (name, item) in config.gateways {
            match RunningSgGateway::create(item, signal.child_token()) {
                Ok(inst) => RunningSgGateway::global_save(name, inst),
                Err(e) => {
                    tracing::error!("[SG.Config] fail to init gateway [{name}]: {e}")
                }
            }
        }
    }
    pub async fn global_reset() {
        let store = Self::global_store();
        let mut task = tokio::task::JoinSet::new();
        {
            let mut g_store = store.lock().expect("poisoned lock");
            for (_, s) in g_store.drain() {
                task.spawn(s.shutdown());
            }
        }
        while let Some(res) = task.join_next().await {
            res.expect("tokio join error")
        }
        PluginRepository::global().clear_instances()
    }

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

    pub async fn global_update(gateway_name: impl AsRef<str>, http_routes: BTreeMap<String, crate::SgHttpRoute>) -> Result<(), BoxError> {
        let gateway_name = gateway_name.as_ref();
        let service = create_router_service(gateway_name.to_string().into(), http_routes)?;
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
    #[instrument(fields(gateway=%config_item.gateway.name), skip_all, err)]
    pub fn create(config_item: ConfigItem, cancel_token: CancellationToken) -> Result<Self, BoxError> {
        let ConfigItem { gateway, routes } = config_item;
        #[allow(unused_mut)]
        // let mut builder_ext = hyper::http::Extensions::new();
        #[cfg(feature = "cache")]
        {
            if let Some(url) = &gateway.parameters.redis_url {
                let url: Arc<str> = url.clone().into();
                // builder_ext.insert(crate::extension::redis_url::RedisUrl(url.clone()));
                // builder_ext.insert(spacegate_kernel::extension::GatewayName(config.gateway.name.clone().into()));
                // Initialize cache instances
                tracing::trace!("Initialize cache client...url:{url}");
                spacegate_ext_redis::RedisClientRepo::global().add(&gateway.name, url.as_ref());
            }
        }
        tracing::info!("[SG.Server] start gateway");
        let reloader = <Reloader<HttpRouterService>>::default();
        let service = create_service(&gateway.name, gateway.plugins, routes, reloader.clone())?;
        if gateway.listeners.is_empty() {
            error!("[SG.Server] Missing Listeners");
        }
        if let Some(_log_level) = gateway.parameters.log_level.clone() {
            // not supported yet

            // tracing::debug!("[SG.Server] change log level to {log_level}");
            // let fw_config = TardisFuns::fw_config();
            // let old_configs = fw_config.log();
            // let directive = format!("{domain}={log_level}", domain = crate::constants::DOMAIN_CODE).parse().expect("invalid directive");
            // let mut directives = old_configs.directives.clone();
            // if let Some(index) = directives.iter().position(|d| d.to_string().starts_with(crate::constants::DOMAIN_CODE)) {
            //     directives.remove(index);
            // }
            // directives.push(directive);
            // TardisFuns::tracing().update_config(&LogConfig {
            //     level: old_configs.level.clone(),
            //     directives,
            //     ..Default::default()
            // })?;
        }

        let gateway_name: Arc<str> = Arc::from(gateway.name.to_string());
        let mut listens: Vec<SgListen<ArcHyperService>> = Vec::new();
        for listener in &gateway.listeners {
            let ip = listener.ip.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
            let addr = SocketAddr::new(ip, listener.port);

            let gateway_name = gateway_name.clone();
            let protocol = listener.protocol.to_string();
            let mut tls_cfg = None;
            if let SgProtocolConfig::Https { ref tls } = listener.protocol {
                tracing::debug!("[SG.Server] Tls is init...mode:{:?}", tls.mode);
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
                            tls_server_cfg.alpn_protocols = vec![b"http/1.1".to_vec(), b"http/1.0".to_vec()];
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

        // let cancel_guard = cancel_token.clone().drop_guard();
        let cancel_task = cancel_token.clone().cancelled_owned();
        let handle = {
            let gateway_name = gateway_name.clone();
            tokio::task::spawn(async move {
                let mut join_set = tokio::task::JoinSet::new();
                for listen in listens {
                    join_set.spawn(async move {
                        let id = listen.listener_id.clone();
                        if let Err(e) = listen.listen().await {
                            tracing::error!("[Sg.Server] listen error: {e}")
                        }
                        tracing::info!("[Sg.Server] listener[{id}] quit listening")
                    });
                }
                tracing::info!(gateway = gateway_name.as_ref(), "[Sg.Server] start all listeners");
                cancel_task.await;
                while let Some(result) = join_set.join_next().await {
                    if let Err(_e) = result {}
                }
                tracing::info!(gateway = gateway_name.as_ref(), "[Sg.Server] cancelled");
            })
        };
        tracing::info!("[SG.Server] start finished");
        Ok(RunningSgGateway {
            gateway_name: gateway_name.clone(),
            token: cancel_token,
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
            tracing::trace!("[SG.Cache] Remove cache client...");
            spacegate_ext_redis::global_repo().remove(name.as_ref());
        }
        match timeout(self.shutdown_timeout, self.handle).await {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("[SG.Server] Wait shutdown timeout:{e}");
            }
        };
        tracing::info!("[SG.Server] Gateway shutdown");
    }
}
