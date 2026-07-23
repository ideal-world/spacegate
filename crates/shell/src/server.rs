use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{Arc, Mutex, OnceLock},
};

use crate::config::{matches_convert::convert_config_to_kernel, plugin_filter_dto::global_batch_mount_plugin, PluginConfig, SgProtocolConfig, SgTlsMode};

use hyper::Version;
use spacegate_config::{
    BackendHost, Config, ConfigItem, McpSessionAffinity, SgBalancePolicy, SgHttpMethodMatch, SgHttpPathMatch, SgHttpRouteMatch, SgMcpRoute, SgMcpTransport, SgRoute, TimeoutMode,
};
use spacegate_kernel::{
    helper_layers::map_request::MapRequestLayer,
    helper_layers::reload::Reloader,
    listener::SgListen,
    service::http_gateway::{builder::default_gateway_route_fallback, create_http_router, HttpRouterService},
    ArcHyperService, BoxError, BoxLayer,
};
use spacegate_plugin::{mount::MountPointIndex, PluginRepository};
use std::time::Duration;
use std::vec::Vec;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};
use tokio_util::sync::CancellationToken;

fn collect_http_route(
    gateway_name: Arc<str>,
    http_routes: impl IntoIterator<Item = (String, SgRoute)>,
) -> Result<HashMap<String, spacegate_kernel::service::http_route::HttpRoute>, BoxError> {
    http_routes
        .into_iter()
        .map(|(name, route)| {
            let (route, mcp_transport) = compile_route(route);
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
                    if let Some(transport) = mcp_transport.clone() {
                        builder = builder.plugin(BoxLayer::new(MapRequestLayer::new(move |mut req: spacegate_kernel::SgRequest| {
                            let session_id_present = req.headers().contains_key("Mcp-Session-Id");
                            req.extensions_mut().insert(spacegate_kernel::extension::McpProxyMeta {
                                transport: transport.clone(),
                                route_type: "MCPRoute",
                                session_id_present,
                            });
                            req
                        })));
                    }
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
                                use spacegate_kernel::helper_layers::map_request::{add_extension::add_extension, MapRequestLayer};
                                use spacegate_kernel::BoxLayer;
                                if let BackendHost::K8sService(ref data) = backend.host {
                                    let namespace_ext = K8sService(data.clone().into());
                                    // need to add to front
                                    builder = builder.plugin(BoxLayer::new(MapRequestLayer::new(add_extension(namespace_ext, true))))
                                }
                            }
                            builder = builder.host(host);
                            if let Some(port) = backend.port {
                                builder = builder.port(port)
                            }
                            if let Some(timeout) = backend.timeout_ms.map(|timeout| Duration::from_millis(timeout as u64)) {
                                builder = builder.timeout(timeout)
                            }
                            if backend.timeout_mode == Some(TimeoutMode::Disabled) {
                                builder = builder.disable_timeout()
                            }
                            let mut layer = if let BackendHost::File { path } = backend.host {
                                builder.file().path(path).build()
                            } else if let Some(protocol) = backend.protocol {
                                builder.schema(protocol.to_string()).build()
                            } else {
                                builder.build()
                            };
                            if backend.downgrade_http2.is_some() {
                                if let spacegate_kernel::service::http_route::Backend::Http { version, .. } = &mut layer.backend {
                                    version.replace(Version::HTTP_11);
                                }
                            }
                            global_batch_mount_plugin(plugins, &mut layer, mount_index);
                            Result::<_, BoxError>::Ok(layer)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    builder = builder.backends(backends);
                    if let Some(timeout) = route_rule.timeout_ms {
                        builder = builder.timeout(Duration::from_millis(timeout as u64));
                    }
                    if route_rule.timeout_mode == Some(TimeoutMode::Disabled) {
                        builder = builder.disable_timeout();
                    }
                    if let Some(policy) = route_rule.balance_policy {
                        builder = builder.balance_policy(match policy {
                            SgBalancePolicy::Random => spacegate_kernel::service::http_route::BalancePolicyEnum::Random,
                            SgBalancePolicy::IpHash => spacegate_kernel::service::http_route::BalancePolicyEnum::IpHash,
                            SgBalancePolicy::McpSession => spacegate_kernel::service::http_route::BalancePolicyEnum::McpSession,
                        });
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

fn compile_route(route: SgRoute) -> (crate::SgHttpRoute, Option<String>) {
    match route {
        SgRoute::Http(route) => (route, None),
        SgRoute::Mcp(route) => compile_mcp_route_to_http_route(route),
    }
}

fn compile_mcp_route_to_http_route(route: SgMcpRoute) -> (crate::SgHttpRoute, Option<String>) {
    let transport = route.transport.clone();
    let matches = match route.transport {
        SgMcpTransport::StreamableHttp => vec![mcp_path_method_match(route.path.clone(), "GET"), mcp_path_method_match(route.path.clone(), "POST")],
        SgMcpTransport::LegacySse => {
            let legacy = route.legacy_sse.unwrap_or_else(|| spacegate_config::SgMcpLegacySse {
                sse_path: "/sse".to_string(),
                message_path: "/message".to_string(),
            });
            vec![mcp_path_method_match(legacy.sse_path, "GET"), mcp_path_method_match(legacy.message_path, "POST")]
        }
    };
    let timeout_mode = route.timeout_mode;
    let session_affinity = route.session_affinity;
    (
        crate::SgHttpRoute {
            route_name: route.route_name,
            hostnames: route.hostnames,
            plugins: route.plugins,
            rules: vec![spacegate_config::SgHttpRouteRule {
                matches: Some(matches),
                plugins: Vec::new(),
                backends: route
                    .backends
                    .into_iter()
                    .map(|mut backend| {
                        backend.timeout_mode.get_or_insert(timeout_mode.clone());
                        backend
                    })
                    .collect(),
                timeout_ms: None,
                timeout_mode: Some(timeout_mode),
                balance_policy: (session_affinity == McpSessionAffinity::McpSession).then_some(spacegate_config::SgBalancePolicy::McpSession),
            }],
            priority: 1,
        },
        Some(match transport {
            SgMcpTransport::StreamableHttp => "streamable_http".to_string(),
            SgMcpTransport::LegacySse => "legacy_sse".to_string(),
        }),
    )
}

fn mcp_path_method_match(path: String, method: impl Into<String>) -> SgHttpRouteMatch {
    SgHttpRouteMatch {
        path: Some(SgHttpPathMatch::Exact { value: path, replace: None }),
        header: None,
        query: None,
        method: Some(vec![SgHttpMethodMatch(method.into())]),
    }
}

/// Create a gateway service from plugins and http_routes
pub(crate) fn create_service(item: ConfigItem, reloader: Reloader<HttpRouterService>) -> Result<ArcHyperService, BoxError> {
    let gateway_name: Arc<str> = item.gateway.name.into();
    let http_routes = item.routes;
    let routes = collect_http_route(gateway_name.clone(), http_routes)?;
    let plugins = item.gateway.plugins.clone();
    let mut builder = spacegate_kernel::service::http_gateway::Gateway::builder(gateway_name.clone());
    if let Some(enable) = item.gateway.parameters.enable_x_request_id {
        builder = builder.x_request_id(enable);
    }
    let mut layer = builder.http_routers(routes).http_route_reloader(reloader).build();
    global_batch_mount_plugin(plugins, &mut layer, MountPointIndex::Gateway { gateway: gateway_name });
    let service = layer.as_service();
    Ok(service)
}

/// create a new sg gateway route, which can be sent to reloader
pub(crate) fn create_router_service(gateway_name: Arc<str>, http_routes: BTreeMap<String, SgRoute>) -> Result<HttpRouterService, BoxError> {
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
            if let Err(err) = PluginRepository::global().create_or_update_instance(PluginConfig {
                id: id.clone(),
                display_name: None,
                spec,
            }) {
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

    pub fn global_update(gateway_name: impl AsRef<str>, http_routes: BTreeMap<String, SgRoute>) -> Result<(), BoxError> {
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
        reloader.reload(service);
        Ok(())
    }
    /// Start a gateway from plugins and http_routes
    #[instrument(fields(gateway=%config_item.gateway.name), skip_all, err)]
    pub fn create(config_item: ConfigItem, cancel_token: CancellationToken) -> Result<Self, BoxError> {
        #[allow(unused_mut)]
        // let mut builder_ext = hyper::http::Extensions::new();
        #[cfg(feature = "cache")]
        {
            if let Some(url) = &config_item.gateway.parameters.redis_url {
                let url: Arc<str> = url.clone().into();
                // builder_ext.insert(crate::extension::redis_url::RedisUrl(url.clone()));
                // builder_ext.insert(spacegate_kernel::extension::GatewayName(config.gateway.name.clone().into()));
                // Initialize cache instances
                tracing::trace!("Initialize cache client...url:{url}");
                spacegate_ext_redis::RedisClientRepo::global().add(&config_item.gateway.name, url.as_ref());
            }
        }
        tracing::info!("[SG.Server] start gateway");
        let reloader = <Reloader<HttpRouterService>>::default();
        let gateway = config_item.gateway.clone();
        let service = create_service(config_item, reloader.clone())?;
        if gateway.listeners.is_empty() {
            error!("[SG.Server] Missing Listeners");
        }

        let gateway_name: Arc<str> = Arc::from(gateway.name.to_string());
        let mut listens: Vec<SgListen> = Vec::new();
        for listener in &gateway.listeners {
            let ip = listener.ip.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
            let addr = SocketAddr::new(ip, listener.port);
            let mut listen = SgListen::new(addr, cancel_token.child_token());
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
                            let _ = rustls::crypto::ring::default_provider().install_default();
                            let builder = rustls::ServerConfig::builder().with_no_client_auth();
                            let mut tls_server_cfg = if let Some(ref host_name) = listener.hostname {
                                info!("Using SNI resolver");
                                let mut resolver = rustls::server::ResolvesServerCertUsingSni::new();
                                let provider = rustls::crypto::CryptoProvider::get_default().expect("should installed");
                                let signed_key = provider.key_provider.load_private_key(key)?;
                                let ck = rustls::sign::CertifiedKey::new(certs, signed_key);
                                resolver.add(host_name, ck)?;
                                builder.with_cert_resolver(Arc::new(resolver))
                            } else {
                                info!("Using single cert");
                                builder.with_single_cert(certs, key)?
                            };
                            tls_server_cfg.alpn_protocols = vec![b"http/1.1".to_vec(), b"h2".to_vec()];
                            tls_server_cfg.ignore_client_order = true;
                            tls_server_cfg.enable_secret_extraction = true;
                            listen.add_service(service.clone().https_with_gateway_name(tls_server_cfg, gateway_name.clone()))
                        } else {
                            error!("[SG.Server] Can not found a valid Tls private key");
                        }
                    };
                }
            } else {
                listen.add_service(service.clone().http_with_gateway_name(gateway_name.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use spacegate_config::{SgBackendRef, SgMcpLegacySse};

    fn backend() -> SgBackendRef {
        SgBackendRef {
            host: BackendHost::Host { host: "127.0.0.1".to_string() },
            port: Some(9001),
            ..Default::default()
        }
    }

    fn match_path_method(route_match: &SgHttpRouteMatch) -> (&str, &str) {
        let path = match route_match.path.as_ref().expect("path match") {
            SgHttpPathMatch::Exact { value, .. } => value.as_str(),
            _ => panic!("MCP route should compile to exact path match"),
        };
        let method = route_match.method.as_ref().expect("method match")[0].0.as_str();
        (path, method)
    }

    #[test]
    fn streamable_http_mcp_route_compiles_to_get_and_post_with_mcp_defaults() {
        let (route, transport) = compile_mcp_route_to_http_route(SgMcpRoute {
            kind: spacegate_config::SgRouteKind::McpRoute,
            route_name: "mcp".to_string(),
            hostnames: None,
            transport: SgMcpTransport::StreamableHttp,
            path: "/mcp".to_string(),
            legacy_sse: None,
            backends: vec![backend()],
            plugins: Vec::new(),
            timeout_mode: TimeoutMode::Disabled,
            session_affinity: McpSessionAffinity::McpSession,
        });

        assert_eq!(transport.as_deref(), Some("streamable_http"));
        let rule = route.rules.first().expect("compiled rule");
        let matches = rule.matches.as_ref().expect("compiled matches");
        assert_eq!(matches.iter().map(match_path_method).collect::<Vec<_>>(), vec![("/mcp", "GET"), ("/mcp", "POST")]);
        assert_eq!(rule.timeout_mode, Some(TimeoutMode::Disabled));
        assert_eq!(rule.backends[0].timeout_mode, Some(TimeoutMode::Disabled));
        assert_eq!(rule.balance_policy, Some(SgBalancePolicy::McpSession));
    }

    #[test]
    fn legacy_sse_mcp_route_compiles_to_sse_get_and_message_post() {
        let (route, transport) = compile_mcp_route_to_http_route(SgMcpRoute {
            kind: spacegate_config::SgRouteKind::McpRoute,
            route_name: "mcp-sse".to_string(),
            hostnames: None,
            transport: SgMcpTransport::LegacySse,
            path: "/ignored".to_string(),
            legacy_sse: Some(SgMcpLegacySse {
                sse_path: "/events".to_string(),
                message_path: "/messages".to_string(),
            }),
            backends: vec![backend()],
            plugins: Vec::new(),
            timeout_mode: TimeoutMode::Disabled,
            session_affinity: McpSessionAffinity::McpSession,
        });

        assert_eq!(transport.as_deref(), Some("legacy_sse"));
        let rule = route.rules.first().expect("compiled rule");
        let matches = rule.matches.as_ref().expect("compiled matches");
        assert_eq!(matches.iter().map(match_path_method).collect::<Vec<_>>(), vec![("/events", "GET"), ("/messages", "POST")]);
    }
}
