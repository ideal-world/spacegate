use std::collections::HashSet;
use std::{cmp::Ordering, collections::HashMap, sync::Arc};

use itertools::Itertools;
use k8s_gateway_api::{Gateway, HttpRoute, HttpRouteFilter};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::ListParams,
    runtime::{watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use spacegate_tower::layers::http_route::match_request::{
    SgHttpHeaderMatch, SgHttpHeaderMatchPolicy, SgHttpMethodMatch, SgHttpPathMatch, SgHttpQueryMatch, SgHttpQueryMatchPolicy, SgHttpRouteMatch,
};
use tardis::regex;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::{future::join_all, pin_mut, TryStreamExt},
    log,
    tokio::sync::RwLock,
    TardisFuns,
};

use crate::update_route;
use crate::{
    constants::{self, GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION},
    do_startup, shutdown,
};

use super::{
    gateway_dto::{SgGateway, SgListener, SgParameters, SgProtocol, SgTlsConfig, SgTlsMode},
    http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
    k8s_crd::SgFilter,
    plugin_filter_dto::SgRouteFilter,
};
use crate::config::k8s_crd_spaceroute::HttpSpaceroute;
use crate::constants::{
    BANCKEND_KIND_EXTERNAL, BANCKEND_KIND_EXTERNAL_HTTP, BANCKEND_KIND_EXTERNAL_HTTPS, GATEWAY_ANNOTATION_LANGUAGE, GATEWAY_ANNOTATION_LOG_LEVEL, GATEWAY_ANNOTATION_REDIS_URL,
};
use crate::helpers::k8s_helper;
use lazy_static::lazy_static;

lazy_static! {
    /// see [SgGateway].name
    /// format see [k8s_helper::format_k8s_obj_unique]
    static ref GATEWAY_UNIQUES: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
}

const GATEWAY_CLASS_NAME: &str = "spacegate";

pub async fn init(namespaces: Option<String>) -> TardisResult<Vec<(SgGateway, Vec<SgHttpRoute>)>> {
    let (gateway_api, http_spaceroute_api, http_route_api, filter_api): (Api<Gateway>, Api<HttpSpaceroute>, Api<HttpRoute>, Api<SgFilter>) = if let Some(namespaces) = namespaces {
        (
            Api::namespaced(get_client().await?, &namespaces),
            Api::namespaced(get_client().await?, &namespaces),
            Api::namespaced(get_client().await?, &namespaces),
            Api::namespaced(get_client().await?, &namespaces),
        )
    } else {
        (
            Api::all(get_client().await?),
            Api::all(get_client().await?),
            Api::all(get_client().await?),
            Api::all(get_client().await?),
        )
    };

    let gateway_objs = gateway_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|gateway_obj| gateway_obj.spec.gateway_class_name == GATEWAY_CLASS_NAME)
        .collect::<Vec<Gateway>>();
    let gateway_objs_versions = k8s_helper::get_obj_uid_version_map(&gateway_objs);

    let mut gateway_objs_params = gateway_objs
        .iter()
        .map(|gateway_obj| (gateway_obj.metadata.uid.clone().unwrap_or("".to_string()), gateway_obj.metadata.annotations.clone()))
        .collect::<HashMap<String, Option<_>>>();

    let gateway_configs = process_gateway_config(gateway_objs.into_iter().collect()).await?;
    let gateway_uniques = gateway_configs.iter().map(|gateway_config| gateway_config.name.clone()).collect::<Vec<String>>();

    let http_route_objs: Vec<HttpSpaceroute> = get_http_spaceroute_by_api(&gateway_uniques, (&http_spaceroute_api, &http_route_api)).await?;

    let http_route_objs_versions = k8s_helper::get_obj_uid_version_map(&http_route_objs);

    let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs.into_iter().collect()).await?;

    let config = gateway_configs
        .into_iter()
        .map(|gateway_config| {
            let http_route_configs: Vec<SgHttpRoute> =
                http_route_configs.iter().filter(|http_route_config| http_route_config.gateway_name == gateway_config.name).cloned().collect::<Vec<SgHttpRoute>>();
            (gateway_config, http_route_configs)
        })
        .collect();

    {
        let mut gateway_uniques_guard = GATEWAY_UNIQUES.write().await;
        *gateway_uniques_guard = gateway_uniques;
    }

    let http_spaceroute_api_clone = http_spaceroute_api.clone();
    let http_route_api_clone = http_route_api.clone();

    // watch gateway
    tardis::tokio::task::spawn_local(async move {
        let ew = watcher::watcher(gateway_api.clone(), watcher::Config::default()).touched_objects();
        pin_mut!(ew);
        while let Some(gateway_obj) = ew.try_next().await.unwrap_or_default() {
            let default_uid = "".to_string();
            let gateway_uid = gateway_obj.metadata.uid.as_ref().unwrap_or(&default_uid);
            if gateway_objs_versions.get(gateway_uid).unwrap_or(&"".to_string()) == &gateway_obj.metadata.resource_version.clone().unwrap_or_default()
                && (gateway_objs_params.get(gateway_uid).unwrap_or(&None) == &gateway_obj.metadata.annotations)
            {
                // ignore the original object
                continue;
            }
            if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                continue;
            }
            gateway_objs_params.insert(gateway_uid.to_string(), gateway_obj.metadata.annotations.clone());

            log::trace!("[SG.Config] Gateway config change found");

            overload_gateway(gateway_obj, (&http_spaceroute_api_clone, &http_route_api_clone)).await;
        }
    });

    async fn watch_http_spaceroute(http_route_obj: HttpSpaceroute, http_route_objs_versions: &HashMap<String, String>, http_route_apis: (&Api<HttpSpaceroute>, &Api<HttpRoute>)) {
        log::trace!("[SG.Config] http_route config watch tiger. name:{}", k8s_helper::get_k8s_obj_unique(&http_route_obj));
        if http_route_objs_versions.get(http_route_obj.metadata.uid.as_ref().unwrap_or(&"".to_string())).unwrap_or(&"".to_string())
            == http_route_obj.metadata.resource_version.as_ref().unwrap_or(&"".to_string())
        {
            let named_http_route_api: Api<HttpRoute> = Api::namespaced(
                get_client().await.expect("[SG.Config] Failed to get client"),
                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
            );
            let named_http_space_route_api: Api<HttpSpaceroute> = Api::namespaced(
                get_client().await.expect("[SG.Config] Failed to get client"),
                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
            );
            if named_http_space_route_api.get(&http_route_obj.name_any()).await.ok().is_some() || named_http_route_api.get(&http_route_obj.name_any()).await.ok().is_some() {
                // ignore the original object
                // ignore if obj is some(it's means obj is not deleted)
                return;
            }
        }
        if http_route_obj.spec.inner.parent_refs.is_none() {
            return;
        }
        let (rel_gateway_namespaces, rel_gateway_name) = (
            if let Some(namespaces) = http_route_obj.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].namespace.as_ref() {
                namespaces.to_string()
            } else {
                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
            },
            http_route_obj.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].name.clone(),
        );
        let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.expect("[SG.Config] Failed to get client"), &rel_gateway_namespaces);
        let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&rel_gateway_name).await {
            if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                return;
            }
            gateway_obj
        } else {
            return;
        };

        log::debug!("[SG.Config] Http route:{} config change found", k8s_helper::get_k8s_obj_unique(&http_route_obj));

        overload_http_route(gateway_obj, (&http_route_apis.0, &http_route_apis.1)).await;
    }

    let http_spaceroute_api_clone = http_spaceroute_api.clone();
    let http_route_api_clone = http_route_api.clone();
    let http_route_apis = (http_spaceroute_api.clone(), http_route_api.clone());

    let http_route_objs_versions_clone = http_route_objs_versions.clone();
    let http_route_apis_clone = http_route_apis.clone();
    // watch http_spaceroute
    tardis::tokio::task::spawn_local(async move {
        watcher::watcher(http_spaceroute_api_clone, watcher::Config::default())
            .touched_objects()
            .default_backoff()
            .try_for_each(|http_route_obj| async {
                watch_http_spaceroute(http_route_obj, &http_route_objs_versions_clone, (&http_route_apis_clone.0, &http_route_apis_clone.1)).await;
                Ok(())
            })
            .await
            .expect("[SG.Config] Watcher try_for_each error");
    });
    // watch  http_route
    tardis::tokio::task::spawn_local(async move {
        watcher::watcher(http_route_api_clone, watcher::Config::default())
            .touched_objects()
            .default_backoff()
            .try_for_each(|http_route_obj| async {
                watch_http_spaceroute(http_route_obj.into(), &http_route_objs_versions, (&http_route_apis.0, &http_route_apis.1)).await;
                Ok(())
            })
            .await
            .expect("[SG.Config] Watcher try_for_each error");
    });

    let sg_filter_objs: Vec<SgFilter> =
        filter_api.list(&ListParams::default()).await.map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?.into_iter().collect();

    let sg_filter_objs_versions = k8s_helper::get_obj_uid_version_map(&sg_filter_objs);

    // watch sgfilter
    tardis::tokio::task::spawn_local(async move {
        let ew = watcher::watcher(filter_api.clone(), watcher::Config::default()).touched_objects();
        pin_mut!(ew);
        while let Some(filter_obj) = ew.try_next().await.unwrap_or_default() {
            log::trace!("[SG.Config] filter_api config watch tiger. name:{}", k8s_helper::get_k8s_obj_unique(&filter_obj));
            if sg_filter_objs_versions.get(filter_obj.metadata.uid.as_ref().unwrap_or(&"".to_string())).unwrap_or(&"".to_string())
                == filter_obj.metadata.resource_version.as_ref().unwrap_or(&"".to_string())
            {
                let named_filter_api: Api<SgFilter> = Api::namespaced(
                    get_client().await.expect("[SG.Config] Failed to get client"),
                    filter_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
                );
                // Do not ignore the deletion event
                if named_filter_api.get(&filter_obj.name_any()).await.ok().is_some() {
                    // ignore the original object
                    continue;
                }
            }
            if filter_obj.spec.target_refs.is_empty() {
                continue;
            }
            let mut gateway_obj_map = HashMap::new();
            let mut http_route_rel_gateway_map = HashMap::new();
            for target_ref in &filter_obj.spec.target_refs {
                if target_ref.kind.eq_ignore_ascii_case("gateway") {
                    let gateway_api: Api<Gateway> = Api::namespaced(
                        get_client().await.expect("[SG.Config] Failed to get client"),
                        target_ref.namespace.as_ref().unwrap_or(&"default".to_string()),
                    );
                    let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&target_ref.name).await {
                        if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                            continue;
                        }
                        gateway_obj
                    } else {
                        continue;
                    };
                    gateway_obj_map.insert(k8s_helper::get_k8s_obj_unique(&gateway_obj), gateway_obj);
                };
                if target_ref.kind.eq_ignore_ascii_case("httpspaceroute") {
                    let http_route_api: Api<HttpSpaceroute> = Api::namespaced(
                        get_client().await.expect("[SG.Config] Failed to get client"),
                        target_ref.namespace.as_ref().unwrap_or(&"default".to_string()),
                    );
                    let (rel_gateway_namespaces, rel_gateway_name) = if let Ok(http_route) = http_route_api.get(&target_ref.name).await {
                        (
                            if let Some(namespaces) =
                                http_route.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].namespace.as_ref()
                            {
                                namespaces.to_string()
                            } else {
                                http_route.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                            },
                            http_route.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].name.clone(),
                        )
                    } else {
                        continue;
                    };
                    let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.expect("[SG.Config] Failed to get client"), &rel_gateway_namespaces);
                    let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&rel_gateway_name).await {
                        if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                            continue;
                        }
                        gateway_obj
                    } else {
                        continue;
                    };
                    let key = k8s_helper::get_k8s_obj_unique(&gateway_obj);
                    if gateway_obj_map.get(&key).is_none() && http_route_rel_gateway_map.get(&key).is_none() {
                        http_route_rel_gateway_map.insert(key, gateway_obj);
                    }
                }
                if target_ref.kind.eq_ignore_ascii_case("httproute") {
                    let http_route_api: Api<HttpRoute> = Api::namespaced(
                        get_client().await.expect("[SG.Config] Failed to get client"),
                        target_ref.namespace.as_ref().unwrap_or(&"default".to_string()),
                    );
                    let (rel_gateway_namespaces, rel_gateway_name) = if let Ok(http_route) = http_route_api.get(&target_ref.name).await {
                        (
                            if let Some(namespaces) =
                                http_route.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].namespace.as_ref()
                            {
                                namespaces.to_string()
                            } else {
                                http_route.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
                            },
                            http_route.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].name.clone(),
                        )
                    } else {
                        continue;
                    };
                    let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.expect("[SG.Config] Failed to get client"), &rel_gateway_namespaces);
                    let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&rel_gateway_name).await {
                        if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
                            continue;
                        }
                        gateway_obj
                    } else {
                        continue;
                    };
                    let key = k8s_helper::get_k8s_obj_unique(&gateway_obj);
                    if gateway_obj_map.get(&key).is_none() && http_route_rel_gateway_map.get(&key).is_none() {
                        http_route_rel_gateway_map.insert(key, gateway_obj);
                    }
                }
            }
            if gateway_obj_map.is_empty() && http_route_rel_gateway_map.is_empty() {
                continue;
            }

            log::trace!("[SG.Config] SgFilter config:{} change found", k8s_helper::get_k8s_obj_unique(&filter_obj));

            let http_route_api = (
                &Api::all(get_client().await.expect("[SG.Config] Failed to get client")),
                &Api::all(get_client().await.expect("[SG.Config] Failed to get client")),
            );
            for gateway_obj in gateway_obj_map.into_values() {
                overload_gateway(gateway_obj, http_route_api).await;
            }

            for gateway_obj in http_route_rel_gateway_map.into_values() {
                overload_http_route(gateway_obj, http_route_api).await;
            }
        }
    });

    Ok(config)
}

async fn get_http_spaceroute_by_api(
    gateway_uniques: &[String],
    (http_spaceroute_api, http_route_api): (&Api<HttpSpaceroute>, &Api<HttpRoute>),
) -> TardisResult<Vec<HttpSpaceroute>> {
    let mut http_route_objs: Vec<HttpSpaceroute> = http_spaceroute_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|http_route_obj| {
            http_route_obj
                .spec
                .inner
                .parent_refs
                .as_ref()
                .map(|parent_refs| {
                    parent_refs.iter().any(|parent_ref| {
                        let http_route_namespace = http_route_obj.namespace();
                        gateway_uniques.contains(&k8s_helper::format_k8s_obj_unique(
                            if let Some(namespaces) = parent_ref.namespace.as_ref() {
                                Some(namespaces)
                            } else {
                                http_route_namespace.as_ref()
                            },
                            &parent_ref.name,
                        ))
                    })
                })
                .unwrap_or(false)
        })
        .collect();
    let http_spaceroute_name_namespace_set =
        http_route_objs.iter().map(|spaceroute| format!("{}{}", spaceroute.name_any(), spaceroute.namespace().unwrap_or_default())).collect::<HashSet<String>>();

    let mut add_http_route_objs: Vec<HttpSpaceroute> = http_route_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|http_route_obj| {
            // HTTPSpaceroute has higher priority than HTTPRoute.
            // HTTPRoute needs to filter already existing HTTPSpaceroute ({name}{namespace} as unique)
            http_spaceroute_name_namespace_set.get(&format!("{}{}", http_route_obj.name_any(), http_route_obj.namespace().unwrap_or_default())).is_none()
                && http_route_obj
                    .spec
                    .inner
                    .parent_refs
                    .as_ref()
                    .map(|parent_refs| {
                        parent_refs.iter().any(|parent_ref| {
                            let http_route_namespace = http_route_obj.namespace();
                            gateway_uniques.contains(&k8s_helper::format_k8s_obj_unique(
                                if let Some(namespaces) = parent_ref.namespace.as_ref() {
                                    Some(namespaces)
                                } else {
                                    http_route_namespace.as_ref()
                                },
                                &parent_ref.name,
                            ))
                        })
                    })
                    .unwrap_or(false)
        })
        .map(|http_route_obj| http_route_obj.into())
        .collect::<Vec<HttpSpaceroute>>();

    http_route_objs.append(&mut add_http_route_objs);

    Ok(http_route_objs)
}

async fn overload_gateway(gateway_obj: Gateway, http_route_api_refs: (&Api<HttpSpaceroute>, &Api<HttpRoute>)) {
    let gateway_unique = k8s_helper::get_k8s_obj_unique(&gateway_obj);
    let gateway_api: Api<Gateway> = Api::namespaced(
        get_client().await.expect("[SG.Config] Failed to get client"),
        gateway_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
    );
    match gateway_api.get_metadata_opt(gateway_obj.metadata.name.as_ref().unwrap_or(&"".to_string())).await {
        Ok(has_gateway_obj) => {
            let gateway_config = process_gateway_config(vec![gateway_obj])
                .await
                .expect("[SG.Config] Failed to process gateway config")
                .first()
                .expect("[SG.Config] Gateway config not found")
                .clone();

            if has_gateway_obj.is_some() {
                {
                    let mut gateway_uniques_guard = GATEWAY_UNIQUES.write().await;
                    gateway_uniques_guard.push(gateway_config.name.clone());
                }
                let http_route_objs: Vec<HttpSpaceroute> = get_http_spaceroute_by_api(&[gateway_unique], http_route_api_refs)
                    .await
                    .map_err(|error| TardisError::wrap(&format!("[SG.Config] Get HttpRoute Kubernetes error: {error:?}"), ""))
                    .expect("");

                let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs.into_iter().collect())
                    .await
                    .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
                    .expect("");
                shutdown(&gateway_config.name).await.expect("[SG.Config] Failed to shutdown gateway");
                log::trace!("[SG.Config] Gateway config change to:{:?}", gateway_config);
                do_startup(gateway_config, http_route_configs).await.expect("[SG.Config] Failed to restart gateway");
            } else {
                {
                    let mut gateway_uniques_guard = GATEWAY_UNIQUES.write().await;
                    gateway_uniques_guard.retain(|name| name != &gateway_config.name);
                }
                shutdown(&gateway_config.name).await.expect("[SG.Config] Failed to shutdown gateway");
            }
        }
        Err(error) => {
            log::warn!("[SG.Config] Gateway config change process error:{error}");
        }
    }
}

// async fn watch_http_spaceroute(
//     http_route_obj: HttpSpaceroute,
//     http_route_objs_versions: &mut HashMap<String, String>,
//     http_route_api_refs: (&Api<HttpSpaceroute>, &Api<HttpRoute>),
// ) {
//     log::trace!("[SG.Config] http_route config watch tiger. name:{}", k8s_helper::get_k8s_obj_unique(&http_route_obj));
//     if http_route_objs_versions.get(http_route_obj.metadata.uid.as_ref().unwrap_or(&"".to_string())).unwrap_or(&"".to_string())
//         == http_route_obj.metadata.resource_version.as_ref().unwrap_or(&"".to_string())
//     {
//         let named_http_route_api: Api<HttpRoute> = Api::namespaced(
//             get_client().await.expect("[SG.Config] Failed to get client"),
//             http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
//         );
//         let named_http_space_route_api: Api<HttpSpaceroute> = Api::namespaced(
//             get_client().await.expect("[SG.Config] Failed to get client"),
//             http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()),
//         );
//         if named_http_space_route_api.get(&http_route_obj.name_any()).await.ok().is_some() || named_http_route_api.get(&http_route_obj.name_any()).await.ok().is_some() {
//             // ignore the original object
//             // ignore if obj is some(it's means obj is not deleted)
//             return;
//         }
//     }
//     if http_route_obj.spec.inner.parent_refs.is_none() {
//         return;
//     }
//     let (rel_gateway_namespaces, rel_gateway_name) = (
//         if let Some(namespaces) = http_route_obj.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].namespace.as_ref() {
//             namespaces.to_string()
//         } else {
//             http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
//         },
//         http_route_obj.spec.inner.parent_refs.as_ref().expect("[SG.Config] http_route not fount parent ref (Gateway)")[0].name.clone(),
//     );
//     let gateway_api: Api<Gateway> = Api::namespaced(get_client().await.expect("[SG.Config] Failed to get client"), &rel_gateway_namespaces);
//     let gateway_obj = if let Ok(Some(gateway_obj)) = gateway_api.get_opt(&rel_gateway_name).await {
//         if gateway_obj.spec.gateway_class_name != GATEWAY_CLASS_NAME {
//             return;
//         }
//         gateway_obj
//     } else {
//         return;
//     };
//
//     log::debug!("[SG.Config] Http route:{} config change found", k8s_helper::get_k8s_obj_unique(&http_route_obj));
//
//     overload_http_route(gateway_obj, http_route_api_refs).await;
// }

async fn overload_http_route(gateway_obj: Gateway, http_route_api_refs: (&Api<HttpSpaceroute>, &Api<HttpRoute>)) {
    let gateway_unique = k8s_helper::get_k8s_obj_unique(&gateway_obj);
    let gateway_config = process_gateway_config(vec![gateway_obj])
        .await
        .expect("[SG.Config] Failed to process gateway config for http_route parent ref")
        .first()
        .expect("[SG.Config] Gateway config not found for http_route parent ref")
        .clone();

    let http_route_objs: Vec<HttpSpaceroute> = get_http_spaceroute_by_api(&[gateway_unique], http_route_api_refs)
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Get HttpRoute Kubernetes error: {error:?}"), ""))
        .expect("");

    if http_route_objs.is_empty() {
        shutdown(&gateway_config.name).await.expect("[SG.Config] Failed to shutdown gateway");
        log::trace!("[SG.Config] Gateway config change to:{:?}", gateway_config);
        do_startup(gateway_config, vec![]).await.expect("[SG.Config] Failed to restart gateway");
    } else {
        let http_route_configs: Vec<SgHttpRoute> = process_http_route_config(http_route_objs).await.expect("[SG.Config] Failed to process http_route config");
        update_route(&gateway_config.name, http_route_configs).await.expect("[SG.Config] Failed to update route config");
    }
}

async fn process_gateway_config(gateway_objs: Vec<Gateway>) -> TardisResult<Vec<SgGateway>> {
    let mut gateway_configs = Vec::new();

    for gateway_obj in gateway_objs {
        // Key configuration compatibility checks
        if gateway_obj.spec.addresses.is_some() {
            return Err(TardisError::not_implemented("[SG.Config] Gateway [spec.addresses] not supported yet", ""));
        }
        if gateway_obj
            .spec
            .listeners
            .iter()
            .any(|listener| !listener.protocol.eq_ignore_ascii_case("https") && !listener.protocol.eq_ignore_ascii_case("http") && !listener.protocol.eq_ignore_ascii_case("ws"))
        {
            return Err(TardisError::not_implemented(
                "[SG.Config] Gateway [spec.listener.protocol!=HTTPS|HTTP|ws] not supported yet",
                "",
            ));
        }
        if gateway_obj
            .spec
            .listeners
            .iter()
            .any(|listener| listener.tls.as_ref().map(|tls| tls.mode.as_ref().map(|mode| !mode.eq_ignore_ascii_case("terminate")).unwrap_or(false)).unwrap_or(false))
        {
            return Err(TardisError::not_implemented(
                "[SG.Config] Gateway [spec.listener.tls.mode!=TERMINATE] not supported yet",
                "",
            ));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| listener.tls.as_ref().map(|tls| tls.options.is_some()).unwrap_or(false)) {
            return Err(TardisError::not_implemented("[SG.Config] Gateway [spec.listener.tls.options] not supported yet", ""));
        }

        // Key configuration legality checks
        if gateway_obj.metadata.name.is_none() {
            return Err(TardisError::format_error("[SG.Config] Gateway [metadata.name] is required", ""));
        }
        if gateway_obj
            .spec
            .listeners
            .iter()
            .any(|listener| (listener.protocol.eq_ignore_ascii_case("https") || listener.protocol.eq_ignore_ascii_case("tls")) && listener.tls.is_none())
        {
            return Err(TardisError::format_error(
                "[SG.Config] Gateway [spec.listener.tls] is required when the Protocol field is “HTTPS” or “TLS”",
                "",
            ));
        }
        if gateway_obj.spec.listeners.iter().any(|listener| {
            listener.tls.is_some()
                && (listener.tls.as_ref().expect("[SG.Config] Unexpected none: listener.tls").certificate_refs.is_none()
                    || listener
                        .tls
                        .as_ref()
                        .expect("[SG.Config] Unexpected none: listener.tls")
                        .certificate_refs
                        .as_ref()
                        .expect("[SG.Config] Unexpected none: listener.tls.certificate_refs")
                        .is_empty())
        }) {
            return Err(TardisError::format_error(
                "[SG.Config] Gateway [spec.listener.tls.certificateRefs] is required when the tls field is enabled",
                "",
            ));
        }
        // Generate gateway configuration
        let gateway_name_without_namespace = gateway_obj.metadata.name.as_ref().ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [metadata.name] is required", ""))?;
        let gateway_config = SgGateway {
            name: k8s_helper::format_k8s_obj_unique(gateway_obj.namespace().as_ref(), gateway_name_without_namespace),
            parameters: SgParameters {
                redis_url: gateway_obj.metadata.annotations.clone().and_then(|ann| ann.get(GATEWAY_ANNOTATION_REDIS_URL).map(|v| v.to_string())),
                log_level: gateway_obj
                    .metadata
                    .annotations
                    .clone()
                    .and_then(|ann: std::collections::BTreeMap<String, String>| ann.get(GATEWAY_ANNOTATION_LOG_LEVEL).map(|v| v.to_string())),
                lang: gateway_obj
                    .metadata
                    .annotations
                    .clone()
                    .and_then(|ann: std::collections::BTreeMap<String, String>| ann.get(GATEWAY_ANNOTATION_LANGUAGE).map(|v| v.to_string())),
                ignore_tls_verification: gateway_obj
                    .metadata
                    .annotations
                    .and_then(|ann: std::collections::BTreeMap<String, String>| ann.get(GATEWAY_ANNOTATION_IGNORE_TLS_VERIFICATION).and_then(|v| v.parse::<bool>().ok())),
            },
            listeners: join_all(
                gateway_obj
                    .spec
                    .listeners
                    .into_iter()
                    .map(|listener| async move {
                        let tls = match listener.tls {
                            Some(tls) => {
                                let certificate_ref = tls
                                    .certificate_refs
                                    .as_ref()
                                    .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is required", ""))?
                                    .first()
                                    .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is empty", ""))?;
                                let secret_api: Api<Secret> = if let Some(namespace) = &certificate_ref.namespace {
                                    Api::namespaced(get_client().await?, namespace)
                                } else {
                                    Api::all(get_client().await?)
                                };
                                let secret_obj =
                                    secret_api.get(&certificate_ref.name).await.map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?;
                                let secret_data = secret_obj
                                    .data
                                    .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data is required", certificate_ref.name), ""))?;
                                let tls_crt = secret_data.get("tls.crt").ok_or_else(|| {
                                    TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.crt] is required", certificate_ref.name), "")
                                })?;
                                let tls_key = secret_data.get("tls.key").ok_or_else(|| {
                                    TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.key] is required", certificate_ref.name), "")
                                })?;
                                Some(SgTlsConfig {
                                    mode: SgTlsMode::from(tls.mode).unwrap_or_default(),
                                    key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
                                    cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
                                })
                            }
                            None => None,
                        };
                        let sg_listener = SgListener {
                            name: Some(listener.name),
                            ip: None,
                            port: listener.port,
                            protocol: match listener.protocol.to_lowercase().as_str() {
                                "http" => SgProtocol::Http,
                                "https" => SgProtocol::Https,
                                "ws" => SgProtocol::Ws,
                                _ => {
                                    return Err(TardisError::not_implemented(
                                        &format!("[SG.Config] Gateway [spec.listener.protocol={}] not supported yet", listener.protocol),
                                        "",
                                    ))
                                }
                            },
                            tls,
                            hostname: listener.hostname,
                        };
                        Ok(sg_listener)
                    })
                    .collect_vec(),
            )
            .await
            .into_iter()
            .map(|listener| listener.expect("[SG.Config] Unexpected none: listener"))
            .collect(),
            filters: get_filters_from_cdr("gateway", gateway_name_without_namespace, &gateway_obj.metadata.namespace).await?,
        };
        gateway_configs.push(gateway_config);
    }
    Ok(gateway_configs)
}

async fn process_http_route_config(mut http_route_objs: Vec<HttpSpaceroute>) -> TardisResult<Vec<SgHttpRoute>> {
    let mut http_route_configs = Vec::new();
    http_route_objs.sort_by(|http_route_a, http_route_b| {
        let (a_priority, b_priority) = (
            http_route_a.annotations().get(constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<i64>().ok()).unwrap_or(0),
            http_route_b.annotations().get(constants::ANNOTATION_RESOURCE_PRIORITY).and_then(|a| a.parse::<i64>().ok()).unwrap_or(0),
        );
        match b_priority.cmp(&a_priority) {
            Ordering::Equal => http_route_a.metadata.creation_timestamp.cmp(&http_route_b.metadata.creation_timestamp),
            _other => _other,
        }
    });

    for http_route_obj in http_route_objs {
        // Key configuration compatibility checks
        if http_route_obj.spec.inner.parent_refs.as_ref().map(|refs| refs.len() > 1).unwrap_or(false) {
            return Err(TardisError::not_implemented(
                "[SG.Config] HttpRoute [spec.parentRefs] multiple values are not supported yet",
                "",
            ));
        }
        if http_route_obj
            .spec
            .rules
            .as_ref()
            .map(|rules| {
                rules.iter().any(|rule| {
                    rule.backend_refs
                        .as_ref()
                        .map(|backends| {
                            backends.iter().any(|backend| {
                                backend.backend_ref.is_some()
                                    && backend
                                        .backend_ref
                                        .as_ref()
                                        .expect("[SG.Config] unexpected none: http_route backendRef")
                                        .inner
                                        .kind
                                        .as_ref()
                                        .map(|kind| {
                                            !kind.eq_ignore_ascii_case("service")
                                                && !kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL)
                                                && !kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL_HTTP)
                                                && !kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL_HTTPS)
                                        })
                                        .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
        {
            return Err(TardisError::not_implemented(
                "[SG.Config] HttpRoute [spec.rules.backendRefs.kind!=(Service || ExternalHttp || ExternalHttps )] not supported yet",
                "",
            ));
        }
        // Key configuration legality checks
        if http_route_obj.spec.inner.parent_refs.is_none() {
            return Err(TardisError::format_error("[SG.Config] HttpRoute [spec.parentRefs] is required", ""));
        }
        // Generate gateway configuration
        let rel_gateway_name = format!(
            "{}.{}",
            if let Some(namespaces) =
                http_route_obj.spec.inner.parent_refs.as_ref().ok_or_else(|| TardisError::format_error("[SG.Config] HttpRoute [spec.parentRefs] is required", ""))?[0]
                    .namespace
                    .as_ref()
            {
                namespaces.to_string()
            } else {
                http_route_obj.namespace().as_ref().unwrap_or(&"default".to_string()).to_string()
            },
            http_route_obj.spec.inner.parent_refs.as_ref().ok_or_else(|| TardisError::format_error("[SG.Config] HttpRoute [spec.parentRefs] is required", ""))?[0].name
        );
        let http_route_config = SgHttpRoute {
            gateway_name: rel_gateway_name,
            hostnames: http_route_obj.spec.hostnames.clone(),
            filters: if let Some(name) = &http_route_obj.metadata.name {
                let kind = if let Some(kind) = http_route_obj.annotations().get(constants::RAW_HTTP_ROUTE_KIND) {
                    kind
                } else {
                    constants::RAW_HTTP_ROUTE_KIND_SPACEROUTE
                };
                get_filters_from_cdr(kind, name, &http_route_obj.metadata.namespace).await?
            } else {
                None
            },
            rules: match http_route_obj.spec.rules {
                Some(rules) => {
                    let sg_rules = rules
                        .into_iter()
                        .map(|rule| SgHttpRouteRule {
                            matches: rule.matches.map(|matches| {
                                matches
                                    .into_iter()
                                    .map(|a_match| SgHttpRouteMatch {
                                        path: a_match.path.and_then(|path| match path {
                                            k8s_gateway_api::HttpPathMatch::Exact { value } => Some(SgHttpPathMatch::Exact(value)),
                                            k8s_gateway_api::HttpPathMatch::PathPrefix { value } => Some(SgHttpPathMatch::Prefix(value)),
                                            k8s_gateway_api::HttpPathMatch::RegularExpression { value } => match regex::Regex::new(&value) {
                                                Ok(re) => Some(SgHttpPathMatch::Regular(re)),
                                                Err(e) => {
                                                    log::error!("[Sg.Init] Invalid regex: {e}");
                                                    None
                                                }
                                            },
                                        }),
                                        header: a_match.headers.map(|headers| {
                                            headers
                                                .into_iter()
                                                .filter_map(|header| match header {
                                                    k8s_gateway_api::HttpHeaderMatch::Exact { name, value } => Some(SgHttpHeaderMatch {
                                                        name,
                                                        policy: SgHttpHeaderMatchPolicy::Exact(value),
                                                    }),
                                                    k8s_gateway_api::HttpHeaderMatch::RegularExpression { name, value } => match regex::Regex::new(&value) {
                                                        Ok(re) => Some(SgHttpHeaderMatch {
                                                            name,
                                                            policy: SgHttpHeaderMatchPolicy::Regular(re),
                                                        }),
                                                        Err(e) => {
                                                            log::error!("[Sg.Init] Invalid regex: {e}");
                                                            None
                                                        }
                                                    },
                                                })
                                                .collect_vec()
                                        }),
                                        query: a_match.query_params.map(|query_params| {
                                            query_params
                                                .into_iter()
                                                .filter_map(|query_param| match query_param {
                                                    k8s_gateway_api::HttpQueryParamMatch::Exact { name, value } => Some(SgHttpQueryMatch {
                                                        name,
                                                        policy: SgHttpQueryMatchPolicy::Exact(value),
                                                    }),
                                                    k8s_gateway_api::HttpQueryParamMatch::RegularExpression { name, value } => match regex::Regex::new(&value) {
                                                        Ok(re) => Some(SgHttpQueryMatch {
                                                            name,
                                                            policy: SgHttpQueryMatchPolicy::Regular(re),
                                                        }),
                                                        Err(e) => {
                                                            log::error!("[Sg.Init] Invalid regex: {e}");
                                                            None
                                                        }
                                                    },
                                                })
                                                .collect_vec()
                                        }),
                                        // ref https://www.rfc-editor.org/rfc/rfc9110.html#name-methods
                                        // Method is case-sensitive and standardized methods are defined in all-uppercase US-ASCII letters
                                        method: a_match.method.map(|m| vec![SgHttpMethodMatch(m)]),
                                    })
                                    .collect_vec()
                            }),
                            filters: convert_filters(rule.filters),
                            backends: rule.backend_refs.map(|backends| {
                                backends
                                    .into_iter()
                                    .map(|backend| {
                                        let filters = convert_filters(backend.filters);
                                        let backend = backend.backend_ref.expect("[SG.Config] unexpected none: http_route backendRef");
                                        let mut protocol = None;
                                        let namespace = match backend.inner.kind {
                                            Some(kind) => {
                                                if kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL) {
                                                    backend.inner.namespace
                                                } else if kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL_HTTP) {
                                                    protocol = Some(SgProtocol::Http);
                                                    backend.inner.namespace
                                                } else if kind.eq_ignore_ascii_case(BANCKEND_KIND_EXTERNAL_HTTPS) {
                                                    protocol = Some(SgProtocol::Https);
                                                    backend.inner.namespace
                                                } else {
                                                    Some(backend.inner.namespace.unwrap_or("default".to_string()))
                                                }
                                            }
                                            None => Some(backend.inner.namespace.unwrap_or("default".to_string())),
                                        };

                                        SgBackendRef {
                                            name_or_host: backend.inner.name,
                                            namespace,
                                            port: backend.inner.port.expect("[SG.Config] unexpected none: http_route backend's port"),
                                            timeout_ms: backend.timeout_ms,
                                            protocol,
                                            weight: backend.weight,
                                            filters,
                                        }
                                    })
                                    .collect_vec()
                            }),
                            timeout_ms: rule.timeout_ms,
                        })
                        .collect_vec();
                    Some(sg_rules)
                }
                None => None,
            },
        };
        http_route_configs.push(http_route_config);
    }
    Ok(http_route_configs)
}

async fn get_filters_from_cdr(kind: &str, name: &str, namespace: &Option<String>) -> TardisResult<Option<Vec<SgRouteFilter>>> {
    let filter_api: Api<SgFilter> = Api::all(get_client().await?);
    let namespace = namespace.clone().unwrap_or("default".to_string());
    let filter_objs: Vec<SgRouteFilter> = filter_api
        .list(&ListParams::default())
        .await
        .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
        .into_iter()
        .filter(|filter_obj| {
            filter_obj.spec.target_refs.iter().any(|target_ref| {
                target_ref.kind.eq_ignore_ascii_case(kind)
                    && target_ref.name.eq_ignore_ascii_case(name)
                    && target_ref.namespace.as_deref().unwrap_or("default").eq_ignore_ascii_case(&namespace)
            })
        })
        .flat_map(|filter_obj| {
            filter_obj.spec.filters.into_iter().filter(|filter| filter.enable).map(|filter| SgRouteFilter {
                code: filter.code,
                name: filter.name,
                spec: filter.config,
            })
        })
        .collect();

    if !filter_objs.is_empty() {
        let mut filter_vec = String::new();
        filter_objs.clone().into_iter().for_each(|filter| filter_vec.push_str(&format!("Filter{{code: {},name:{}}},", filter.code, filter.name.unwrap_or("None".to_string()))));
        log::trace!("[SG.Config.SgFilter] {namespace}.{kind}.{name} filter found: {}", filter_vec.trim_end_matches(','));
    }

    if filter_objs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(filter_objs))
    }
}

fn convert_filters(filters: Option<Vec<HttpRouteFilter>>) -> Option<Vec<SgRouteFilter>> {
    filters
        .map(|filters| {
            filters
                .into_iter()
                .map(|filter| {
                    use spacegate_plugin::{model::*, plugins::*};
                    let sg_filter = match filter {
                        k8s_gateway_api::HttpRouteFilter::RequestHeaderModifier { request_header_modifier } => {
                            let mut sg_sets = HashMap::new();
                            if let Some(adds) = request_header_modifier.add {
                                for add in adds {
                                    sg_sets.insert(add.name, add.value);
                                }
                            }
                            if let Some(sets) = request_header_modifier.set {
                                for set in sets {
                                    sg_sets.insert(set.name, set.value);
                                }
                            }
                            SgRouteFilter {
                                code: spacegate_plugin::plugins::header_modifier::CODE.to_string(),
                                name: None,
                                spec: TardisFuns::json.obj_to_json(&header_modifier::SgFilterHeaderModifier {
                                    kind: header_modifier::SgFilterHeaderModifierKind::Request,
                                    sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                                    remove: request_header_modifier.remove,
                                })?,
                            }
                        }
                        k8s_gateway_api::HttpRouteFilter::ResponseHeaderModifier { response_header_modifier } => {
                            let mut sg_sets = HashMap::new();
                            if let Some(adds) = response_header_modifier.add {
                                for add in adds {
                                    sg_sets.insert(add.name, add.value);
                                }
                            }
                            if let Some(sets) = response_header_modifier.set {
                                for set in sets {
                                    sg_sets.insert(set.name, set.value);
                                }
                            }
                            SgRouteFilter {
                                code: header_modifier::CODE.to_string(),
                                name: None,
                                spec: TardisFuns::json.obj_to_json(&header_modifier::SgFilterHeaderModifier {
                                    kind: header_modifier::SgFilterHeaderModifierKind::Response,
                                    sets: if sg_sets.is_empty() { None } else { Some(sg_sets) },
                                    remove: response_header_modifier.remove,
                                })?,
                            }
                        }
                        k8s_gateway_api::HttpRouteFilter::RequestRedirect { request_redirect } => SgRouteFilter {
                            code: redirect::CODE.to_string(),
                            name: None,
                            spec: TardisFuns::json.obj_to_json(&redirect::RedirectFilter {
                                scheme: request_redirect.scheme,
                                hostname: request_redirect.hostname,
                                path: request_redirect.path.map(|path| match path {
                                    k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
                                        kind: SgHttpPathModifierType::ReplaceFullPath,
                                        value: replace_full_path,
                                    },
                                    k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
                                        kind: SgHttpPathModifierType::ReplacePrefixMatch,
                                        value: replace_prefix_match,
                                    },
                                }),
                                port: request_redirect.port,
                                status_code: request_redirect.status_code,
                            })?,
                        },
                        k8s_gateway_api::HttpRouteFilter::URLRewrite { url_rewrite } => SgRouteFilter {
                            code: rewrite::CODE.to_string(),
                            name: None,
                            spec: TardisFuns::json.obj_to_json(&rewrite::SgFilterRewriteConfig {
                                hostname: url_rewrite.hostname,
                                path: url_rewrite.path.map(|path| match path {
                                    k8s_gateway_api::HttpPathModifier::ReplaceFullPath { replace_full_path } => SgHttpPathModifier {
                                        kind: SgHttpPathModifierType::ReplaceFullPath,
                                        value: replace_full_path,
                                    },
                                    k8s_gateway_api::HttpPathModifier::ReplacePrefixMatch { replace_prefix_match } => SgHttpPathModifier {
                                        kind: SgHttpPathModifierType::ReplacePrefixMatch,
                                        value: replace_prefix_match,
                                    },
                                }),
                            })?,
                        },
                        k8s_gateway_api::HttpRouteFilter::RequestMirror { .. } => {
                            return Err(TardisError::not_implemented(
                                "[SG.Config] HttpRoute [spec.rules.filters.type=RequestMirror] not supported yet",
                                "",
                            ))
                        }
                        k8s_gateway_api::HttpRouteFilter::ExtensionRef { .. } => {
                            return Err(TardisError::not_implemented(
                                "[SG.Config] HttpRoute [spec.rules.filters.type=ExtensionRef] not supported yet",
                                "",
                            ))
                        }
                    };
                    Ok(sg_filter)
                })
                .collect_vec()
        })
        .map(|filters| filters.into_iter().map(|filter| filter.expect("Unreachable code")).collect_vec())
}

async fn get_client() -> TardisResult<Client> {
    Client::try_default().await.map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))
}
