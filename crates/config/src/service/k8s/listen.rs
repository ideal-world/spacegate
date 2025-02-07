use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    task::ready,
};

use futures_util::{pin_mut, TryStreamExt};
use k8s_gateway_api::{Gateway, HttpRoute};
use kube::{
    api::ObjectMeta,
    runtime::{watcher, WatchStreamExt},
    Api, Resource, ResourceExt,
};
use spacegate_model::{
    constants,
    ext::k8s::crd::{
        http_spaceroute::HttpSpaceroute,
        sg_filter::{K8sSgFilterSpecTargetRef, SgFilter},
    },
    BoxResult, Config, PluginInstanceId,
};

use crate::service::{k8s::convert::filter_k8s_conv::PluginIdConv, ConfigEventType, ConfigType, CreateListener, Listen, ListenEvent, Retrieve as _};

use super::K8s;

pub struct K8sListener {
    rx: tokio::sync::mpsc::UnboundedReceiver<(ConfigType, ConfigEventType)>,
}
impl K8sListener {}

impl K8s {
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
}

impl CreateListener for K8s {
    const CONFIG_LISTENER_NAME: &'static str = "k8s";
    type Listener = K8sListener;
    async fn create_listener(&self) -> BoxResult<(Config, Self::Listener)> {
        let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel();

        // self.reject_gateway_class(constants::GATEWAY_CLASS_NAME).await?;
        let config = self.retrieve_config().await?;
        // self.accept_gateway_class(constants::GATEWAY_CLASS_NAME).await?;

        let gateway_api: Api<Gateway> = self.get_namespace_api();
        let http_route_api: Api<HttpRoute> = self.get_namespace_api();
        let http_spaceroute_api: Api<HttpSpaceroute> = self.get_namespace_api();
        let sg_filter_api: Api<SgFilter> = self.get_namespace_api();

        let move_gateway_names = config.gateways.clone().into_values().map(|item| item.gateway.name).collect::<Vec<_>>();
        let move_evt_tx = evt_tx.clone();
        tokio::task::spawn(async move {
            let mut gateway_uid_version_map = HashMap::new();

            let apply_event = |gateway: Gateway, mut gateway_uid_version_map: HashMap<_, _>| -> HashMap<_, _> {
                if move_gateway_names.contains(&gateway.name_any()) && !gateway_uid_version_map.contains_key(&gateway.uid()) {
                    // ignore existing obj
                    gateway_uid_version_map.insert(gateway.uid(), gateway.meta().clone());
                    return gateway_uid_version_map;
                }
                if gateway_uid_version_map.get(&gateway.uid()).map(|gateway_meta| &gateway_meta.resource_version) == Some(&gateway.resource_version()) {
                    // ignore same version obj
                    return gateway_uid_version_map;
                }
                if gateway.spec.gateway_class_name != constants::GATEWAY_CLASS_NAME {
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

        let move_filter_codes_names = config
            .gateways
            .clone()
            .into_values()
            .flat_map(|item| {
                let mut plugin_ids = item.gateway.plugins.clone();
                let route_plugin_ids = item.routes.values().flat_map(|route| route.plugins.clone()).collect::<Vec<_>>();
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
                match target_digest_map.get(&name_any) {
                    Some(d) if *d == digest => continue,
                    _ => {
                        if filter.spec.target_refs.is_empty() && !target_ref_map.contains_key(&name_any) {
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

        let listener = K8sListener { rx: evt_rx };

        Ok((config, listener))
    }
}

impl Listen for K8sListener {
    fn poll_next(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<BoxResult<ListenEvent>> {
        if let Some(next) = ready!(self.rx.poll_recv(cx)) {
            std::task::Poll::Ready(Ok(next.into()))
        } else {
            std::task::Poll::Pending
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
