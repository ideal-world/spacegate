use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{Mutex, OnceLock},
};

use crate::config::{
    matches_convert::convert_config_to_kernel,
    plugin_filter_dto::{global_batch_mount_plugin, global_batch_update_plugin},
    SgProtocolConfig, SgRouteFilter, SgTlsMode,
};

use lazy_static::lazy_static;
use spacegate_config::ConfigItem;
use spacegate_kernel::{
    helper_layers::reload::Reloader,
    layers::gateway::{builder::default_gateway_route_fallback, create_http_router, SgGatewayRoute},
    listener::SgListen,
    service::get_http_backend_service,
    ArcHyperService, BoxError, Layer,
};
use spacegate_plugin::{mount::MountPointIndex, SgPluginRepository};
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use tokio::time::timeout;
use tokio::{self, sync::watch::Sender, task::JoinHandle};
use tracing::{debug, error, info, instrument, warn};

use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};
use tokio_util::sync::CancellationToken;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
    static ref START_JOIN_HANDLE: Arc<Mutex<HashMap<String, JoinHandle<()>>>> = <_>::default();
}

fn collect_http_route(
    gateway_name: Arc<str>,
    http_routes: impl IntoIterator<Item = (String, crate::SgHttpRoute)>,
) -> Result<HashMap<String, spacegate_kernel::layers::http_route::SgHttpRoute>, BoxError> {
    http_routes
        .into_iter()
        .map(|(name, route)| {
            let route_name: Arc<str> = name.clone().into();
            let mount_index = MountPointIndex::HttpRoute {
                gateway: gateway_name.clone(),
                route: route_name.clone(),
            };
            let plugins = route.filters;
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
                    let mut builder = spacegate_kernel::layers::http_route::SgHttpRouteRuleLayer::builder();
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
                            let mut builder = spacegate_kernel::layers::http_route::SgHttpBackendLayer::builder();
                            let plugins = backend.filters;
                            #[cfg(feature = "k8s")]
                            {
                                use crate::extension::k8s_service::K8sService;
                                use spacegate_config::model::BackendHost;
                                use spacegate_kernel::helper_layers::map_request::{add_extension::add_extension, MapRequestLayer};
                                use spacegate_kernel::SgBoxLayer;
                                if let BackendHost::K8sService(data) = backend.host {
                                    let namespace_ext = K8sService(data.into());
                                    builder = builder.plugin(SgBoxLayer::new(MapRequestLayer::new(add_extension(namespace_ext, true))))
                                }
                            }
                            builder = builder.host(host).port(backend.port);
                            if let Some(timeout) = backend.timeout_ms.map(|timeout| Duration::from_millis(timeout as u64)) {
                                builder = builder.timeout(timeout)
                            }
                            if let Some(protocol) = backend.protocol {
                                builder = builder.protocol(protocol.to_string());
                            }
                            let mut layer = builder.build()?;
                            global_batch_mount_plugin(plugins, &mut layer, mount_index);
                            Result::<_, BoxError>::Ok(layer)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    builder = builder.backends(backends);
                    if let Some(timeout) = route_rule.timeout_ms {
                        builder = builder.timeout(Duration::from_millis(timeout as u64));
                    }
                    let mut layer = builder.build()?;
                    global_batch_mount_plugin(route_rule.filters, &mut layer, mount_index);
                    Result::<_, BoxError>::Ok(layer)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut layer =
                spacegate_kernel::layers::http_route::SgHttpRoute::builder().hostnames(route.hostnames.unwrap_or_default()).rules(rules).priority(route.priority).build()?;
            global_batch_mount_plugin(plugins, &mut layer, mount_index);
            Ok((name, layer))
        })
        .collect::<Result<HashMap<String, _>, _>>()
}

/// Create a gateway service from plugins and http_routes
pub(crate) fn create_service(
    gateway_name: &str,
    plugins: Vec<SgRouteFilter>,
    http_routes: BTreeMap<String, crate::SgHttpRoute>,
    reloader: Reloader<SgGatewayRoute>,
) -> Result<ArcHyperService, BoxError> {
    let gateway_name: Arc<str> = gateway_name.into();
    let routes = collect_http_route(gateway_name.clone(), http_routes)?;
    let mut layer = spacegate_kernel::layers::gateway::SgGatewayLayer::builder(gateway_name.clone()).http_routers(routes).http_route_reloader(reloader).build();
    global_batch_mount_plugin(plugins, &mut layer, MountPointIndex::Gateway { gateway: gateway_name });
    let backend_service = get_http_backend_service();
    let service = ArcHyperService::new(layer.layer(backend_service));
    Ok(service)
}

/// create a new sg gateway route, which can be sent to reloader
pub(crate) fn create_router_service(gateway_name: Arc<str>, http_routes: BTreeMap<String, crate::SgHttpRoute>) -> Result<SgGatewayRoute, BoxError> {
    let routes = collect_http_route(gateway_name, http_routes.clone())?;
    let service = create_http_router(routes.values(), &default_gateway_route_fallback(), get_http_backend_service());
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
        SgPluginRepository::global().clear_gateway_instances(gateway_name.as_ref());
        let mut global_store = global_store.lock().expect("poisoned lock");
        global_store.remove(gateway_name.as_ref())
    }

    pub async fn global_update(gateway_name: impl AsRef<str>, http_routes: BTreeMap<String, crate::SgHttpRoute>) -> Result<(), BoxError> {
        let gateway_name = gateway_name.as_ref();
        SgPluginRepository::global().clear_routes_instances(gateway_name);
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
        global_batch_update_plugin(config_item.collect_all_plugins());
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
        let reloader = <Reloader<SgGatewayRoute>>::default();
        let service = create_service(&gateway.name, gateway.filters, routes, reloader.clone())?;
        if gateway.listeners.is_empty() {
            return Err("[SG.Server] Missing Listeners".into());
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
            let ip = listener.ip.unwrap_or(std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED));
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
                    tracing::error!("[Sg.Server] listen error: {e}")
                }
                tracing::info!("[Sg.Server] listener[{id}] quit listening")
            });
        }

        // let cancel_guard = cancel_token.clone().drop_guard();
        let cancel_task = cancel_token.clone().cancelled_owned();
        let handle = {
            let gateway_name = gateway_name.clone();
            tokio::task::spawn_local(async move {
                tracing::info!(gateway = gateway_name.as_ref(), "[Sg.Server] start all listeners");
                local_set.run_until(cancel_task).await;
                tracing::info!(gateway = gateway_name.as_ref(), "[Sg.Server] cancelled");
            })
        };
        tracing::info!("[SG.Server] start finished");
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
