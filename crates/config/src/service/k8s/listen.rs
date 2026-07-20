use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    task::ready,
};

use futures_util::{pin_mut, TryStreamExt};
use k8s_gateway_api::{Gateway, HttpRoute};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{ObjectMeta, PostParams},
    runtime::{watcher, WatchStreamExt},
    Api, Resource, ResourceExt,
};
use spacegate_model::{
    ext::k8s::crd::{
        http_spaceroute::HttpSpaceroute,
        mcp_route::McpRoute,
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
        wasm_plugin::{HigressWasmPluginStatus, WasmPlugin},
    },
    BoxResult, Config, PluginInstanceId,
};
use tracing::debug;

use crate::service::{
    k8s::{
        convert::{filter_k8s_conv::PluginIdConv, higress_wasm_plugin_conv::HigressWasmPluginConv as _},
        retrieve::oci_auth_from_secret,
    },
    ConfigEventType, ConfigType, CreateListener, Listen, ListenEvent, Retrieve as _,
};

use super::{gateway_uses_class, K8s};

pub struct K8sListener {
    rx: tokio::sync::mpsc::UnboundedReceiver<(ConfigType, ConfigEventType)>,
}
impl K8sListener {}

impl K8s {
    async fn reconcile_wasm_plugin_status(api: &Api<WasmPlugin>, secret_api: &Api<Secret>, plugin: &WasmPlugin) {
        let (phase, message) = match Self::validate_wasm_plugin(api, secret_api, plugin).await {
            Ok(()) => ("Accepted".to_string(), "WasmPlugin accepted by Spacegate".to_string()),
            Err(e) => ("Unsupported".to_string(), e),
        };
        let status = HigressWasmPluginStatus {
            observed_generation: plugin.meta().generation,
            phase: Some(phase),
            digest: plugin.digest(),
            message: Some(message),
        };
        let mut update = plugin.clone();
        update.status = Some(status);
        if let Err(e) = api.replace_status(&plugin.name_any(), &PostParams::default(), serde_json::to_vec(&update).unwrap_or_default()).await {
            tracing::warn!(name = %plugin.name_any(), error = %e, "failed to update WasmPlugin status");
        }
    }

    async fn validate_wasm_plugin(_api: &Api<WasmPlugin>, secret_api: &Api<Secret>, plugin: &WasmPlugin) -> Result<(), String> {
        plugin.validate_for_spacegate()?;
        let Some(registry) = plugin.oci_registry() else {
            return Ok(());
        };
        let Some(secret_name) = plugin.spec.image_pull_secret.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
            return Ok(());
        };
        let secret = secret_api
            .get_opt(secret_name)
            .await
            .map_err(|e| format!("read imagePullSecret {secret_name}: {e}"))?
            .ok_or_else(|| format!("imagePullSecret {secret_name} not found"))?;
        if oci_auth_from_secret(&secret, &registry).is_none() {
            return Err(format!("imagePullSecret {secret_name} does not contain credentials for {registry}"));
        }
        Ok(())
    }

    async fn process_http_spaceroute_event(
        move_evt_tx: &tokio::sync::mpsc::UnboundedSender<(ConfigType, ConfigEventType)>,
        move_http_route_names: &[String],
        move_namespace: &str,
        http_route_event: watcher::Event<HttpSpaceroute>,
        uid_version_map: &mut HashMap<Option<String>, ObjectMeta>,
    ) {
        let apply_event = |http_route: HttpSpaceroute, uid_version_map: &mut HashMap<_, _>| {
            if move_http_route_names.contains(&http_route.name_any()) && uid_version_map.get(&http_route.uid()).is_none() {
                // ignore existing obj
                uid_version_map.insert(http_route.uid(), http_route.meta().clone());
                return;
            }
            if uid_version_map.get(&http_route.uid()) == Some(http_route.meta()) {
                // ignore same version obj
                return;
            }
            move_evt_tx
                .send((
                    ConfigType::Route {
                        name: http_route.name_any(),
                        gateway_name: http_route.get_gateway_name(move_namespace),
                    },
                    ConfigEventType::Delete,
                ))
                .expect("send event error");
        };
        match http_route_event {
            watcher::Event::Applied(http_route) => apply_event(http_route, uid_version_map),
            watcher::Event::Deleted(http_route) => {
                uid_version_map.remove(&http_route.uid());
                move_evt_tx
                    .send((
                        ConfigType::Route {
                            name: http_route.name_any(),
                            gateway_name: http_route.get_gateway_name(move_namespace),
                        },
                        ConfigEventType::Delete,
                    ))
                    .expect("send event error");
            }
            watcher::Event::Restarted(http_routes) => {
                // Should be used as a signal to replace the store contents atomically.
                let mut uid_version_map_clone = uid_version_map.clone();
                *uid_version_map = HashMap::new();

                for http_route in http_routes {
                    apply_event(http_route.clone(), uid_version_map);
                    uid_version_map_clone.remove(&http_route.uid());
                }

                uid_version_map_clone.into_values().for_each(|meta| {
                    move_evt_tx
                        .send((
                            ConfigType::Gateway {
                                name: meta.name.unwrap_or_default(),
                            },
                            ConfigEventType::Delete,
                        ))
                        .expect("send event error")
                });
            }
        }
    }

    async fn process_mcp_route_event(
        move_evt_tx: &tokio::sync::mpsc::UnboundedSender<(ConfigType, ConfigEventType)>,
        move_mcp_route_names: &[String],
        move_namespace: &str,
        mcp_route_event: watcher::Event<McpRoute>,
        uid_version_map: &mut HashMap<Option<String>, ObjectMeta>,
    ) {
        let apply_event = |mcp_route: McpRoute, uid_version_map: &mut HashMap<_, _>| {
            if move_mcp_route_names.contains(&mcp_route.name_any()) && uid_version_map.get(&mcp_route.uid()).is_none() {
                uid_version_map.insert(mcp_route.uid(), mcp_route.meta().clone());
                return;
            }
            if uid_version_map.get(&mcp_route.uid()) == Some(mcp_route.meta()) {
                return;
            }
            move_evt_tx
                .send((
                    ConfigType::Route {
                        name: mcp_route.name_any(),
                        gateway_name: mcp_route.get_gateway_name(move_namespace),
                    },
                    ConfigEventType::Delete,
                ))
                .expect("send event error");
        };
        match mcp_route_event {
            watcher::Event::Applied(mcp_route) => apply_event(mcp_route, uid_version_map),
            watcher::Event::Deleted(mcp_route) => {
                uid_version_map.remove(&mcp_route.uid());
                move_evt_tx
                    .send((
                        ConfigType::Route {
                            name: mcp_route.name_any(),
                            gateway_name: mcp_route.get_gateway_name(move_namespace),
                        },
                        ConfigEventType::Delete,
                    ))
                    .expect("send event error");
            }
            watcher::Event::Restarted(mcp_routes) => {
                for mcp_route in mcp_routes {
                    apply_event(mcp_route, uid_version_map);
                }
            }
        }
    }
}

impl CreateListener for K8s {
    const CONFIG_LISTENER_NAME: &'static str = "k8s";
    type Listener = K8sListener;
    async fn create_listener(&self) -> BoxResult<(Config, Self::Listener)> {
        let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel();

        let config = self.retrieve_config().await?;
        self.accept_gateway_class(self.gateway_class_name.as_ref()).await?;

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        let http_route_api: Api<HttpRoute> = self.get_namespace_api();
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let mcp_route_api: Api<McpRoute> = self.get_namespace_api();
        let sg_filter_api: Api<SgFilter> = self.get_namespace_api();
        let wasm_plugin_api: Api<WasmPlugin> = self.get_namespace_api();
        let secret_api: Api<Secret> = self.get_namespace_api();

        let move_gateway_names = config.gateways.clone().into_values().map(|item| item.gateway.name).collect::<Vec<_>>();
        let move_gateway_class_name = self.gateway_class_name.clone();
        let move_evt_tx = evt_tx.clone();
        #[cfg(unix)]
        {
            let move_evt_tx = move_evt_tx.clone();
            if let Ok(mut hangup_signal_listen) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
                tokio::task::spawn(async move {
                    tracing::info!("start hangup signal listening");
                    loop {
                        hangup_signal_listen.recv().await;
                        move_evt_tx.send((ConfigType::Global, ConfigEventType::Update)).expect("send event error");
                    }
                });
            }
        }
        tokio::task::spawn(async move {
            let mut gateway_uid_version_map = HashMap::new();

            let apply_event = |gateway: Gateway, mut gateway_uid_version_map: HashMap<_, _>| -> HashMap<_, _> {
                if !gateway_uses_class(&gateway, &move_gateway_class_name) {
                    return gateway_uid_version_map;
                }
                if move_gateway_names.contains(&gateway.name_any()) && !gateway_uid_version_map.contains_key(&gateway.uid()) {
                    // ignore existing obj
                    gateway_uid_version_map.insert(gateway.uid(), gateway.meta().clone());
                    return gateway_uid_version_map;
                }
                if gateway_uid_version_map.get(&gateway.uid()).map(|gateway_meta| &gateway_meta.resource_version) == Some(&gateway.resource_version()) {
                    // ignore same version obj
                    return gateway_uid_version_map;
                }
                gateway_uid_version_map.insert(gateway.uid(), gateway.meta().clone());

                tracing::debug!("[SG.Config] Gateway config change found");

                move_evt_tx.send((ConfigType::Gateway { name: gateway.name_any() }, ConfigEventType::Update)).expect("send event error");
                gateway_uid_version_map
            };

            let ew = watcher::watcher(gateway_api.clone(), watcher::Config::default());
            pin_mut!(ew);
            while let Some(gateway_event) = ew.try_next().await.unwrap_or_default() {
                match gateway_event {
                    watcher::Event::Applied(gateway) => {
                        gateway_uid_version_map = apply_event(gateway, gateway_uid_version_map);
                    }
                    watcher::Event::Deleted(gateway) => {
                        if !gateway_uses_class(&gateway, &move_gateway_class_name) {
                            continue;
                        }
                        gateway_uid_version_map.remove(&gateway.uid());
                        move_evt_tx.send((ConfigType::Gateway { name: gateway.name_any() }, ConfigEventType::Delete)).expect("send event error");
                    }
                    watcher::Event::Restarted(gateways) => {
                        // Should be used as a signal to replace the store contents atomically.
                        let mut gateway_uid_version_map_clone = gateway_uid_version_map.clone();
                        gateway_uid_version_map = HashMap::new();

                        for gateway in gateways {
                            gateway_uid_version_map = apply_event(gateway.clone(), gateway_uid_version_map);
                            gateway_uid_version_map_clone.remove(&gateway.uid());
                        }

                        gateway_uid_version_map_clone.into_values().for_each(|meta| {
                            move_evt_tx
                                .send((
                                    ConfigType::Gateway {
                                        name: meta.name.unwrap_or_default(),
                                    },
                                    ConfigEventType::Delete,
                                ))
                                .expect("send event error")
                        });
                    }
                }
            }
        });

        let move_http_route_names = config.gateways.clone().into_values().flat_map(|item| item.routes.keys().cloned().collect::<Vec<_>>()).collect::<Vec<_>>();
        let move_evt_tx = evt_tx.clone();
        let move_namespace = self.namespace.to_string();
        let move_http_spaceroute_api = http_spaceroute_api.clone();
        //watch http spaceroute
        tokio::task::spawn(async move {
            let mut uid_version_map = HashMap::new();
            let ew = watcher::watcher(move_http_spaceroute_api, watcher::Config::default());
            pin_mut!(ew);
            while let Some(http_route_event) = ew.try_next().await.unwrap_or_default() {
                Self::process_http_spaceroute_event(&move_evt_tx, &move_http_route_names, &move_namespace, http_route_event, &mut uid_version_map).await
            }
        });

        let move_http_route_names = config.gateways.clone().into_values().flat_map(|item| item.routes.keys().cloned().collect::<Vec<_>>()).collect::<Vec<_>>();
        let move_evt_tx = evt_tx.clone();
        let move_namespace = self.namespace.to_string();
        let move_http_route_api = http_route_api.clone();
        //watch http route
        tokio::task::spawn(async move {
            let mut uid_version_map = HashMap::new();
            let ew = watcher::watcher(move_http_route_api, watcher::Config::default());
            pin_mut!(ew);
            while let Some(http_route_event) = ew.try_next().await.unwrap_or_default() {
                Self::process_http_spaceroute_event(
                    &move_evt_tx,
                    &move_http_route_names,
                    &move_namespace,
                    http_route_event.map(|route| route.into()),
                    &mut uid_version_map,
                )
                .await
            }
        });

        let move_mcp_route_names = config.gateways.clone().into_values().flat_map(|item| item.routes.keys().cloned().collect::<Vec<_>>()).collect::<Vec<_>>();
        let move_evt_tx = evt_tx.clone();
        let move_namespace = self.namespace.to_string();
        let move_mcp_route_api = mcp_route_api.clone();
        tokio::task::spawn(async move {
            let mut uid_version_map = HashMap::new();
            let ew = watcher::watcher(move_mcp_route_api, watcher::Config::default());
            pin_mut!(ew);
            while let Some(mcp_route_event) = ew.try_next().await.unwrap_or_default() {
                Self::process_mcp_route_event(&move_evt_tx, &move_mcp_route_names, &move_namespace, mcp_route_event, &mut uid_version_map).await
            }
        });

        let move_filter_codes_names = config
            .gateways
            .clone()
            .into_values()
            .flat_map(|item| {
                let mut plugin_ids = item.gateway.plugins.clone();
                let route_plugin_ids = item
                    .routes
                    .values()
                    .flat_map(|route| match route {
                        spacegate_model::SgRoute::Http(route) => route.plugins.clone(),
                        spacegate_model::SgRoute::Mcp(route) => route.plugins.clone(),
                    })
                    .collect::<Vec<_>>();
                plugin_ids.extend(route_plugin_ids);
                plugin_ids
            })
            .map(|f| (f.code, f.name))
            .collect::<Vec<_>>();
        let move_evt_tx = evt_tx.clone();
        let move_namespace = self.namespace.to_string();

        //watch sgfilter
        tokio::task::spawn(async move {
            let mut uid_version_map = HashMap::new();
            let mut target_digest_map: HashMap<String, u64> = HashMap::new();
            let mut target_ref_map: HashMap<String, Vec<K8sSgFilterSpecTargetRef>> = HashMap::new();
            let ew = watcher::watcher(sg_filter_api, watcher::Config::default()).touched_objects();
            pin_mut!(ew);
            while let Some(filter) = ew.try_next().await.unwrap_or_default() {
                let name_any = filter.name_any();
                if filter.spec.filters.iter().any(|inner_filter| move_filter_codes_names.contains(&(inner_filter.code.clone().into(), inner_filter.name.clone().into())))
                    && !uid_version_map.contains_key(&name_any)
                {
                    uid_version_map.insert(name_any.clone(), filter.resource_version());
                    continue;
                }
                if uid_version_map.get(&name_any) == Some(&filter.resource_version()) {
                    continue;
                }

                for p in &filter.spec.filters {
                    if p.enable {
                        let id = PluginInstanceId::extract_from_filter(p, &name_any);
                        move_evt_tx.send((ConfigType::Plugin { id }, ConfigEventType::Update)).expect("send event error");
                    }
                }

                let digest = {
                    let mut hasher = std::hash::DefaultHasher::new();
                    filter.spec.target_refs.hash(&mut hasher);
                    hasher.finish()
                };
                debug!("filter {} - new digest: {}, old digest: {:?}", name_any, digest, target_digest_map.get(&name_any));
                match target_digest_map.get(&name_any) {
                    Some(d) if *d == digest => continue,
                    _ => {
                        if filter.spec.target_refs.is_empty() && !target_ref_map.contains_key(&name_any) {
                            debug!("skip empty target_refs for filter {}", name_any);
                            continue;
                        }
                        target_digest_map.insert(name_any.clone(), digest);
                    }
                }

                let update_set: HashSet<_> = filter.spec.target_refs.iter().collect();
                let old_set: HashSet<_> = target_ref_map.get(&name_any).map(|old| old.iter().collect()).unwrap_or_default();

                let add_vec: Vec<_> = update_set.difference(&old_set).collect();
                let mut delete_vec: Vec<_> = old_set.difference(&update_set).collect();

                let mut updated_vec = add_vec;
                updated_vec.append(&mut delete_vec);

                debug!("target_refs changes - update_set: {:?}, old_set: {:?}, updated_vec: {:?}", update_set, old_set, updated_vec);
                for target_ref in updated_vec {
                    match target_ref.kind.as_str() {
                        "Gateway" => {
                            move_evt_tx.send((ConfigType::Gateway { name: target_ref.name.clone() }, ConfigEventType::Update)).expect("send event error");
                        }
                        "HTTPRoute" | "HTTPSpaceroute" => {
                            let target_route: Option<HttpSpaceroute> = if let Ok(Some(http_route)) = http_spaceroute_api.get_opt(&target_ref.name).await {
                                Some(http_route)
                            } else if let Ok(Some(http_route)) = http_route_api.get_opt(&target_ref.name).await {
                                Some(http_route.into())
                            } else {
                                None
                            };
                            if let Some(target_route) = target_route {
                                move_evt_tx
                                    .send((
                                        ConfigType::Route {
                                            gateway_name: target_route.get_gateway_name(&move_namespace),
                                            name: target_ref.name.clone(),
                                        },
                                        ConfigEventType::Update,
                                    ))
                                    .expect("send event error");
                            }
                        }
                        _ => {}
                    }
                }
                if filter.spec.target_refs.is_empty() {
                    target_ref_map.remove(&name_any);
                } else {
                    target_ref_map.insert(name_any, filter.spec.target_refs);
                }
            }
        });

        let move_evt_tx = evt_tx.clone();
        let wasm_plugin_status_api = wasm_plugin_api.clone();
        let wasm_plugin_secret_api = secret_api.clone();
        // watch Higress-compatible WasmPlugin. A WasmPlugin can add/remove gateway-level
        // plugins, so the simplest correct reconciliation is a global reload.
        tokio::task::spawn(async move {
            let mut uid_version_map = HashMap::new();
            let ew = watcher::watcher(wasm_plugin_api, watcher::Config::default());
            pin_mut!(ew);
            while let Some(event) = ew.try_next().await.unwrap_or_default() {
                match event {
                    watcher::Event::Applied(plugin) => {
                        Self::reconcile_wasm_plugin_status(&wasm_plugin_status_api, &wasm_plugin_secret_api, &plugin).await;
                        if uid_version_map.get(&plugin.uid()) == Some(plugin.meta()) {
                            continue;
                        }
                        uid_version_map.insert(plugin.uid(), plugin.meta().clone());
                        move_evt_tx
                            .send((
                                ConfigType::Plugin {
                                    id: plugin.to_spacegate_plugin_id(),
                                },
                                ConfigEventType::Update,
                            ))
                            .expect("send event error");
                        move_evt_tx.send((ConfigType::Global, ConfigEventType::Update)).expect("send event error");
                    }
                    watcher::Event::Deleted(plugin) => {
                        uid_version_map.remove(&plugin.uid());
                        move_evt_tx
                            .send((
                                ConfigType::Plugin {
                                    id: plugin.to_spacegate_plugin_id(),
                                },
                                ConfigEventType::Delete,
                            ))
                            .expect("send event error");
                        move_evt_tx.send((ConfigType::Global, ConfigEventType::Update)).expect("send event error");
                    }
                    watcher::Event::Restarted(plugins) => {
                        for plugin in &plugins {
                            Self::reconcile_wasm_plugin_status(&wasm_plugin_status_api, &wasm_plugin_secret_api, plugin).await;
                        }
                        uid_version_map = plugins.into_iter().map(|plugin| (plugin.uid(), plugin.meta().clone())).collect();
                        move_evt_tx.send((ConfigType::Global, ConfigEventType::Update)).expect("send event error");
                    }
                }
            }
        });

        let listener = K8sListener { rx: evt_rx };

        Ok((config, listener))
    }
}

impl Listen for K8sListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<BoxResult<ListenEvent>> {
        if let Some(next) = ready!(self.rx.poll_recv(cx)) {
            std::task::Poll::Ready(Ok(next.into()))
        } else {
            std::task::Poll::Ready(Err("k8s event listener sender shutdown".into()))
        }
    }
}

trait EnumMap<K, T, F> {
    fn map(self, f: F) -> watcher::Event<T>
    where
        F: FnMut(K) -> T;
}
impl<K, T, F> EnumMap<K, T, F> for watcher::Event<K> {
    fn map(self, mut f: F) -> watcher::Event<T>
    where
        F: FnMut(K) -> T,
    {
        match self {
            watcher::Event::Applied(k) => watcher::Event::Applied(f(k)),
            watcher::Event::Deleted(k) => watcher::Event::Deleted(f(k)),
            watcher::Event::Restarted(k) => watcher::Event::Restarted(k.into_iter().map(f).collect()),
        }
    }
}
