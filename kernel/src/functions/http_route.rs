use std::{collections::HashMap, net::SocketAddr};

use crate::{
    config::{
        gateway_dto::{SgGateway, SgListener, SgProtocol},
        http_route_dto::{SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType, SgHttpRoute},
    },
    plugins::{
        context::{ChoseHttpRouteRuleInst, SgRouteFilterRequestAction, SgRoutePluginContext},
        filters::{self, BoxSgPluginFilter, SgPluginFilterInitDto},
    },
};
use http::{header::UPGRADE, Request, Response};
use hyper::{client::HttpConnector, Body, Client, StatusCode};
use hyper_rustls::HttpsConnector;
use itertools::Itertools;
use std::sync::Arc;
use std::vec::Vec;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log,
    rand::{distributions::WeightedIndex, prelude::Distribution, thread_rng},
    regex::Regex,
};

use super::http_client;

static mut ROUTES: Option<HashMap<String, SgGatewayInst>> = None;

pub async fn init(gateway_conf: SgGateway, routes: Vec<SgHttpRoute>) -> TardisResult<()> {
    let _all_route_rules = routes.iter().flat_map(|route| route.rules.clone().unwrap_or_default()).collect::<Vec<_>>();
    let global_filters = if let Some(filters) = gateway_conf.clone().filters {
        filters::init(filters, SgPluginFilterInitDto::from_global(&gateway_conf, &routes)).await?
    } else {
        Vec::new()
    };
    let mut route_insts = Vec::new();
    for route in routes.clone() {
        let route_filters = if let Some(filters) = route.clone().filters {
            filters::init(filters, SgPluginFilterInitDto::from_route(&gateway_conf, &route)).await?
        } else {
            Vec::new()
        };
        let rule_insts = if let Some(rules) = route.rules {
            let mut rule_insts = Vec::new();
            for rule in rules {
                let rule_filters = if let Some(filters) = rule.filters.clone() {
                    filters::init(filters, SgPluginFilterInitDto::from_rule_or_backend(&gateway_conf, &rule)).await?
                } else {
                    Vec::new()
                };

                let rule_matches_insts = rule
                    .clone()
                    .matches
                    .map(|rule_matches| {
                        rule_matches
                            .into_iter()
                            .map(|rule_match| {
                                let path_inst = rule_match
                                    .path
                                    .map(|path| {
                                        let regular = if path.kind == SgHttpPathMatchType::Regular {
                                            Regex::new(&path.value)
                                                .map_err(|_| TardisError::format_error(&format!("[SG.Route] Path Regular {} format error", path.value), ""))
                                                .map(Some)?
                                        } else {
                                            None
                                        };
                                        Ok::<_, TardisError>(SgHttpPathMatchInst {
                                            regular,
                                            kind: path.kind,
                                            value: path.value,
                                        })
                                    })
                                    .transpose()?;

                                let header_inst = rule_match
                                    .header
                                    .map(|header| {
                                        header
                                            .into_iter()
                                            .map(|header| {
                                                let regular = if header.kind == SgHttpHeaderMatchType::Regular {
                                                    Some(
                                                        Regex::new(&header.value)
                                                            .map_err(|_| TardisError::format_error(&format!("[SG.Route] Header Regular {} format error", header.value), ""))?,
                                                    )
                                                } else {
                                                    None
                                                };
                                                Ok(SgHttpHeaderMatchInst {
                                                    regular,
                                                    kind: header.kind,
                                                    name: header.name.clone(),
                                                    value: header.value,
                                                })
                                            })
                                            .collect::<TardisResult<Vec<SgHttpHeaderMatchInst>>>()
                                    })
                                    .transpose()?;

                                let query_inst = rule_match
                                    .query
                                    .map(|query| {
                                        query
                                            .into_iter()
                                            .map(|query| {
                                                let regular = if query.kind == SgHttpQueryMatchType::Regular {
                                                    Some(
                                                        Regex::new(&query.value)
                                                            .map_err(|_| TardisError::format_error(&format!("[SG.Route] Query Regular {} format error", query.value), ""))?,
                                                    )
                                                } else {
                                                    None
                                                };
                                                Ok(SgHttpQueryMatchInst {
                                                    regular,
                                                    kind: query.kind,
                                                    name: query.name.clone(),
                                                    value: query.value,
                                                })
                                            })
                                            .collect::<TardisResult<Vec<SgHttpQueryMatchInst>>>()
                                    })
                                    .transpose()?;

                                Ok(SgHttpRouteMatchInst {
                                    path: path_inst,
                                    header: header_inst,
                                    query: query_inst,
                                    method: rule_match.method.map(|m| m.into_iter().map(|m| m.to_lowercase()).collect_vec()),
                                })
                            })
                            .collect::<TardisResult<Vec<SgHttpRouteMatchInst>>>()
                    })
                    .transpose()?;
                let gateway_conf_clone = Arc::new(gateway_conf.clone());
                rule_insts.push(SgHttpRouteRuleInst {
                    filters: rule_filters,
                    matches: rule_matches_insts,
                    backends: if let Some(backend_refs) = rule.clone().backends {
                        let backends = join_all(
                            backend_refs
                                .into_iter()
                                .map(|backend_ref| (backend_ref, &rule))
                                .map(move |(backend_ref, read_only_route)| {
                                    let gateway_conf_clone = gateway_conf_clone.clone();
                                    async move {
                                        let filters = if let Some(filters) = backend_ref.clone().filters {
                                            filters::init(filters, SgPluginFilterInitDto::from_backend(&gateway_conf_clone, read_only_route, &backend_ref)).await?
                                        } else {
                                            Vec::new()
                                        };
                                        Ok::<_, TardisError>(SgBackendInst {
                                            name_or_host: backend_ref.name_or_host,
                                            namespace: backend_ref.namespace,
                                            port: backend_ref.port,
                                            timeout_ms: backend_ref.timeout_ms,
                                            protocol: backend_ref.protocol,
                                            weight: backend_ref.weight,
                                            filters,
                                        })
                                    }
                                })
                                .collect_vec(),
                        )
                        .await;
                        Some(backends.into_iter().collect::<Result<Vec<_>, _>>()?)
                    } else {
                        None
                    },
                    timeout_ms: rule.timeout_ms,
                })
            }
            Ok::<_, TardisError>(Some(rule_insts))
        } else {
            Ok(None)
        }?;
        route_insts.push(SgHttpRouteInst {
            hostnames: route.hostnames.map(|hostnames| hostnames.into_iter().map(|hostname| hostname.to_lowercase()).collect_vec()),
            filters: route_filters,
            rules: rule_insts,
        })
    }

    log::trace!(
        "[SG.Route] Init matched rule {:?} by  {}",
        route_insts
            .iter()
            .map(|route| route
                .rules
                .as_ref()
                .map(|r| r
                    .iter()
                    .map(|r| if let Some(matchs) = r.matches.as_ref() {
                        matchs.iter().map(|m| format!("{:?}", m)).collect::<Vec<_>>().join(", ")
                    } else {
                        "None".to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", "))
                .unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", "),
        gateway_conf.name
    );

    let route_inst = SgGatewayInst {
        filters: global_filters,
        routes: route_insts,
        client: http_client::init()?,
        listeners: gateway_conf.listeners,
    };
    unsafe {
        if ROUTES.is_none() {
            ROUTES = Some(HashMap::new());
        }
        ROUTES.as_mut().expect("Unreachable code").insert(gateway_conf.name.to_string(), route_inst);
    };

    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    let route = unsafe {
        if ROUTES.is_none() {
            ROUTES = Some(HashMap::new());
        }
        ROUTES.as_mut().expect("Unreachable code").remove(name)
    };
    if let Some(gateway_inst) = route {
        for (_, filter) in gateway_inst.filters {
            filter.destroy().await?;
        }
        for route in gateway_inst.routes {
            for (_, filter) in route.filters {
                filter.destroy().await?;
            }
            if let Some(rules) = route.rules {
                for rule in rules {
                    for (_, filter) in rule.filters {
                        filter.destroy().await?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn get(name: &str) -> TardisResult<&'static SgGatewayInst> {
    unsafe {
        if let Some(routes) = ROUTES.as_ref().ok_or_else(|| TardisError::bad_request("[SG.Route] Get routes failed", ""))?.get(name) {
            Ok(routes)
        } else {
            Err(TardisError::bad_request(&format!("[SG.Route] Get routes {name} failed"), ""))
        }
    }
}

pub async fn process(gateway_name: Arc<String>, req_scheme: &str, (remote_addr, local_addr): (SocketAddr, SocketAddr), mut request: Request<Body>) -> TardisResult<Response<Body>> {
    if request.uri().host().is_none() && request.headers().contains_key("Host") {
        *request.uri_mut() = format!(
            "{}://{}{}",
            req_scheme,
            request
                .headers()
                .get("Host")
                .ok_or_else(|| TardisError::bad_request("[SG.Route] request get Host failed", ""))?
                .to_str()
                .map_err(|_| TardisError::bad_request("[SG.Route] request host illegal: host is not ascii", ""))?,
            request.uri()
        )
        .parse()
        .map_err(|e| TardisError::bad_request(&format!("[SG.Route] request host rebuild illegal: {}", e), ""))?;
    }
    log::trace!(
        "[SG.Route] Request method {} url {}, request addr {}, from {} @ {}",
        request.method(),
        request.uri(),
        local_addr,
        remote_addr,
        gateway_name
    );
    let gateway_inst = get(&gateway_name)?;
    if !match_listeners_hostname_and_port(request.uri().host(), local_addr.port(), &gateway_inst.listeners) {
        log::trace!("[SG.Route] Request hostname {} not match", request.uri().host().expect(""));
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        return Ok(not_found);
    }

    let (matched_route_inst, matched_rule_inst, matched_match_inst) = match_route_process(&request, &gateway_inst.routes);

    if matched_route_inst.is_none() {
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        return Ok(not_found);
    };

    let matched_route_inst = matched_route_inst.expect("Unreachable code");

    let backend = if let Some(Some(backends)) = matched_rule_inst.map(|rule| &rule.backends) {
        choose_backend(backends)
    } else {
        None
    };

    let backend_filters = backend.map(|backend| &backend.filters);
    let rule_filters = matched_rule_inst.map(|rule| &rule.filters);

    if request.headers().get(UPGRADE).map(|v| &v.to_str().expect("[SG.Websocket] Upgrade header value illegal:  is not ascii").to_lowercase() == "websocket").unwrap_or(false) {
        #[cfg(feature = "ws")]
        {
            if let Some(backend) = backend {
                log::trace!("[SG.Route] Backend: {:?}", backend.name_or_host);
                let mut ctx = process_req_filters_ws(
                    gateway_name.to_string(),
                    remote_addr,
                    &request,
                    backend_filters,
                    rule_filters,
                    &matched_route_inst.filters,
                    &gateway_inst.filters,
                    matched_rule_inst,
                    matched_match_inst,
                )
                .await?;
                *request.uri_mut() = ctx.get_req_uri().clone();
                return crate::functions::websocket::process(gateway_name, remote_addr, backend, request).await;
            } else {
                return Err(TardisError::bad_request(
                    &format!("[SG.Websocket] No backend found , from {remote_addr} @ {gateway_name}"),
                    "",
                ));
            }
        }
        #[cfg(not(feature = "ws"))]
        {
            return Err(TardisError::bad_request(
                &format!("[SG.Websocket] Websocket function is not enabled , from {remote_addr} @ {gateway_name}"),
                "",
            ));
        }
    }

    let ctx = process_req_filters_http(
        gateway_name.to_string(),
        remote_addr,
        request,
        backend_filters,
        rule_filters,
        &matched_route_inst.filters,
        &gateway_inst.filters,
        matched_rule_inst,
        matched_match_inst,
    )
    .await?;

    let ctx = if ctx.get_action() == &SgRouteFilterRequestAction::Response {
        ctx
    } else {
        let rule_timeout = if let Some(matched_rule_inst) = matched_rule_inst {
            matched_rule_inst.timeout_ms
        } else {
            None
        };

        log::trace!(
            "[SG.Request] matched rule: {}, backend: {:?}",
            if let Some(matched_rule) = matched_rule_inst.and_then(|r| r.matches.as_ref()) {
                matched_rule.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>().join(", ")
            } else {
                "None".to_string()
            },
            backend.map(|b| b.name_or_host.clone())
        );
        http_client::request(&gateway_inst.client, backend, rule_timeout, ctx.get_action() == &SgRouteFilterRequestAction::Redirect, ctx).await?
    };

    let mut ctx: SgRoutePluginContext = process_resp_filters(ctx, backend_filters, rule_filters, &matched_route_inst.filters, &gateway_inst.filters, matched_match_inst).await?;

    ctx.build_response().await
}

///Match route by SgHttpRouteInst list
/// First, we perform route matching based on the hostname. Hostname matching can fall into three categories:
/// exact domain name match, wildcard domain match, and unspecified domain name match.
/// The priority of matching rules decreases in the following order: exact > wildcard > unspecified.
/// If there are higher-priority domain name matching rules, the routing will always be matched from those rules.
/// For example, if we have the following matching rules
///  - "a.example.com" /
///  - "a.example.com" /iam
///  - "*.example.com" /iam/b
///  The path to be matched: "a.example.com/iam/b"
///  only the second rule will be considered, because it has a higher priority than the others.
///
/// List of hostname matching rules
/// 1. Exact domain match: "example.com" -> Handles exact hostname "example.com"
/// 2. Wildcard domain match: "*.example.com" -> Handles any subdomain of "example.com"
/// 3. Unspecified domain match: "*" -> Handles any hostname not matched by the above rules
fn match_route_process<'a>(
    req: &Request<Body>,
    routes: &'a Vec<SgHttpRouteInst>,
) -> (Option<&'a SgHttpRouteInst>, Option<&'a SgHttpRouteRuleInst>, Option<&'a SgHttpRouteMatchInst>) {
    let (highest, second, lowest) = match_route_insts_with_hostname_priority(req.uri().host(), routes);
    let matched_hostname_route_priorities = vec![highest, second, lowest];

    let mut matched_route_inst = None;
    let mut matched_rule_inst = None;
    let mut matched_match_inst = None;
    let mut matched = false;
    for matched_hostname_route_priority in matched_hostname_route_priorities {
        let mut first_matched_route_inst = None;
        let mut first_matched_rule_inst = None;
        for matched_hostname_route in matched_hostname_route_priority {
            if let Some(rule_insts) = &matched_hostname_route.rules {
                for rule_inst in rule_insts {
                    (matched, matched_match_inst) = match_rule_inst(req, rule_inst.matches.as_ref());
                    if matched {
                        if matched_match_inst.is_some() {
                            matched_rule_inst = Some(rule_inst);
                            break;
                        };
                        if first_matched_rule_inst.is_none() {
                            first_matched_rule_inst = Some(rule_inst);
                        }
                    }
                }
                if matched_rule_inst.is_none() {
                    matched_rule_inst = first_matched_rule_inst;
                }
                if matched {
                    if matched_match_inst.is_some() {
                        matched_route_inst = Some(matched_hostname_route);
                        break;
                    };
                    if first_matched_route_inst.is_none() {
                        first_matched_route_inst = Some(matched_hostname_route);
                    }
                }
            } else if first_matched_route_inst.is_none() {
                first_matched_route_inst = Some(matched_hostname_route);
            };
            if matched && matched_match_inst.is_some() {
                matched_route_inst = Some(matched_hostname_route);
                break;
            }
        }
        if first_matched_route_inst.is_some() && matched_route_inst.is_none() {
            matched_route_inst = first_matched_route_inst;
        }
        if matched_route_inst.is_some() {
            break;
        }
    }

    (matched_route_inst, matched_rule_inst, matched_match_inst)
}

///Filter according to the hostname of route, and return with priority in sequence.
/// first return is the highest priority, only exact match sequence
/// second return is the second priority, only wildcard match sequence
/// last return is the lowest priority, only no hostname specified sequence
fn match_route_insts_with_hostname_priority<'a>(
    req_host: Option<&str>,
    routes: &'a Vec<SgHttpRouteInst>,
) -> (Vec<&'a SgHttpRouteInst>, Vec<&'a SgHttpRouteInst>, Vec<&'a SgHttpRouteInst>) {
    if let Some(req_host) = req_host {
        let mut highest_priority_route = Vec::new();
        let mut matched_route_by_wildcard = Vec::new();
        let mut matched_route_by_no_set = Vec::new();
        for route_inst in routes {
            if let Some(hostnames) = &route_inst.hostnames {
                if hostnames.iter().any(|hostname| hostname == req_host) {
                    // Exact match, highest priority
                    highest_priority_route.push(route_inst);
                } else {
                    //start fuzzy match
                    if hostnames.iter().any(|hostname| hostname == &"*".to_string()) {
                        // hostname = * ,equal to No Hostname specified , lowest priority
                        matched_route_by_no_set.push(route_inst);
                        continue;
                    }
                    let req_host_item = req_host.split('.').collect::<Vec<&str>>();
                    if hostnames.iter().any(|hostname| {
                        let hostname_item = hostname.split('.').collect::<Vec<&str>>();
                        if hostname_item.len() == req_host_item.len() {
                            hostname_item.iter().zip(req_host_item.iter()).all(|(hostname_item, req_host_item)| hostname_item == req_host_item || hostname_item == &"*")
                        } else {
                            false
                        }
                    }) {
                        // Fuzzy match, the second priority
                        matched_route_by_wildcard.push(route_inst);
                    }
                }
            } else {
                // No Hostname specified, equal to *, lowest priority
                matched_route_by_no_set.push(route_inst)
            }
        }
        (highest_priority_route, matched_route_by_wildcard, matched_route_by_no_set)
    } else {
        (Vec::new(), Vec::new(), routes.iter().filter(|route_inst| route_inst.hostnames.is_none()).collect())
    }
}

fn match_rule_inst<'a>(req: &Request<Body>, rule_matches: Option<&'a Vec<SgHttpRouteMatchInst>>) -> (bool, Option<&'a SgHttpRouteMatchInst>) {
    if let Some(matches) = rule_matches {
        for rule_match in matches {
            if let Some(method) = &rule_match.method {
                if !method.contains(&req.method().as_str().to_lowercase()) {
                    continue;
                }
            }
            if let Some(path) = &rule_match.path {
                let req_path = req.uri().path();
                match path.kind {
                    SgHttpPathMatchType::Exact => {
                        if req_path != path.value {
                            continue;
                        }
                    }
                    SgHttpPathMatchType::Prefix => {
                        if !req_path.starts_with(&path.value) {
                            continue;
                        }
                    }
                    SgHttpPathMatchType::Regular => {
                        if !&path.regular.as_ref().expect("[SG.Route] Path regular is None").is_match(req_path) {
                            continue;
                        }
                    }
                }
            }
            if let Some(headers) = &rule_match.header {
                let matched = headers.iter().all(|header| {
                    if let Some(req_header_value) = req.headers().get(&header.name) {
                        if req_header_value.is_empty() {
                            return false;
                        }
                        let req_header_value = req_header_value.to_str();
                        if req_header_value.is_err() {
                            return false;
                        }
                        let req_header_value = req_header_value.expect("Unreachable code");
                        match header.kind {
                            SgHttpHeaderMatchType::Exact => {
                                if req_header_value != header.value {
                                    return false;
                                }
                            }
                            SgHttpHeaderMatchType::Regular => {
                                if !&header.regular.as_ref().expect("Unreachable code").is_match(req_header_value) {
                                    return false;
                                }
                            }
                        }
                    } else {
                        return false;
                    }
                    true
                });
                if !matched {
                    continue;
                }
            }
            if let Some(queries) = &rule_match.query {
                let matched = queries.iter().all(|query| {
                    if let Some(Some(req_query_value)) = req.uri().query().map(|q| {
                        let q = urlencoding::decode(q).expect("[SG.Route] urlencoding decode error");
                        let q = q.as_ref().split('&').collect_vec();
                        q.into_iter().map(|item| item.split('=').collect_vec()).find(|item| item.len() == 2 && item[0] == query.name).map(|item| item[1].to_string())
                    }) {
                        match query.kind {
                            SgHttpQueryMatchType::Exact => {
                                if req_query_value != query.value {
                                    return false;
                                }
                            }
                            SgHttpQueryMatchType::Regular => {
                                if !&query.regular.as_ref().expect("[SG.Route] query regular is None").is_match(&req_query_value) {
                                    return false;
                                }
                            }
                        }
                    } else {
                        return false;
                    }
                    true
                });
                if !matched {
                    continue;
                }
            }
            return (true, Some(rule_match));
        }
        (false, None)
    } else {
        (true, None)
    }
}

fn match_listeners_hostname_and_port(hostname: Option<&str>, port: u16, listeners: &[SgListener]) -> bool {
    if let Some(hostname) = hostname {
        listeners
            .iter()
            .filter(|listener| {
                (if let Some(listener_hostname) = listener.hostname.clone() {
                    if listener_hostname == *hostname {
                        true
                    } else if let Some(stripped) = listener_hostname.strip_prefix("*.") {
                        hostname.ends_with(stripped) && hostname != stripped
                    } else {
                        false
                    }
                } else {
                    true
                }) && listener.port == port
            })
            .count()
            > 0
    } else {
        true
    }
}

async fn process_req_filters_http(
    gateway_name: String,
    remote_addr: SocketAddr,
    request: Request<Body>,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_rule_inst: Option<&SgHttpRouteRuleInst>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRoutePluginContext> {
    let ctx = SgRoutePluginContext::new_http(
        request.method().clone(),
        request.uri().clone(),
        request.version(),
        request.headers().clone(),
        request.into_body(),
        remote_addr,
        gateway_name,
        matched_rule_inst.map(|m| ChoseHttpRouteRuleInst::clone_from(m, matched_match_inst)),
    );
    process_req_filters(ctx, backend_filters, rule_filters, route_filters, global_filters, matched_match_inst).await
}

#[cfg(feature = "ws")]
async fn process_req_filters_ws(
    gateway_name: String,
    remote_addr: SocketAddr,
    request: &Request<Body>,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_rule_inst: Option<&SgHttpRouteRuleInst>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRoutePluginContext> {
    let ctx = SgRoutePluginContext::new_ws(
        request.method().clone(),
        request.uri().clone(),
        request.version(),
        request.headers().clone(),
        remote_addr,
        gateway_name,
        matched_rule_inst.map(|m| ChoseHttpRouteRuleInst::clone_from(m, matched_match_inst)),
    );
    process_req_filters(ctx, backend_filters, rule_filters, route_filters, global_filters, matched_match_inst).await
}

async fn process_req_filters(
    mut ctx: SgRoutePluginContext,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRoutePluginContext> {
    let mut is_continue;
    let mut executed_filters = Vec::new();
    if let Some(backend_filters) = backend_filters {
        for (id, filter) in backend_filters {
            if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
                log::trace!("[SG.Plugin.Filter] Hit id {id} in request");
                (is_continue, ctx) = filter.req_filter(id, ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(id);
            }
        }
    }
    if let Some(rule_filters) = rule_filters {
        for (id, filter) in rule_filters {
            if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
                log::trace!("[SG.Plugin.Filter] Hit id {id} in request");
                (is_continue, ctx) = filter.req_filter(id, ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(id);
            }
        }
    }
    for (id, filter) in route_filters {
        if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
            log::trace!("[SG.Plugin.Filter] Hit id {id} in request");
            (is_continue, ctx) = filter.req_filter(id, ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(id);
        }
    }
    for (id, filter) in global_filters {
        if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
            log::trace!("[SG.Plugin.Filter] Hit id {id} in request");
            (is_continue, ctx) = filter.req_filter(id, ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(id);
        }
    }
    Ok(ctx)
}

async fn process_resp_filters(
    mut ctx: SgRoutePluginContext,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRoutePluginContext> {
    let mut is_continue;
    let mut executed_filters = Vec::new();

    if let Some(backend_filters) = backend_filters {
        for (id, filter) in backend_filters {
            if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
                log::trace!("[SG.Plugin.Filter] Hit id {id} in response");
                (is_continue, ctx) = filter.resp_filter(id, ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(id);
            }
        }
    }
    if let Some(rule_filters) = rule_filters {
        for (id, filter) in rule_filters {
            if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
                log::trace!("[SG.Plugin.Filter] Hit id {id} in response");
                (is_continue, ctx) = filter.resp_filter(id, ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(id);
            }
        }
    }
    for (id, filter) in route_filters {
        if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
            log::trace!("[SG.Plugin.Filter] Hit id {id} in response");
            (is_continue, ctx) = filter.resp_filter(id, ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(id);
        }
    }
    for (id, filter) in global_filters {
        if !executed_filters.contains(&id) && filter.before_resp_filter_check(&ctx) {
            log::trace!("[SG.Plugin.Filter] Hit id {id} in response");
            (is_continue, ctx) = filter.resp_filter(id, ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(id);
        }
    }
    Ok(ctx)
}

fn choose_backend(backends: &Vec<SgBackendInst>) -> Option<&SgBackendInst> {
    if backends.is_empty() {
        None
    } else if backends.len() == 1 {
        backends.get(0)
    } else {
        let weights = backends.iter().map(|backend| backend.weight.unwrap_or(0)).collect_vec();
        let dist = WeightedIndex::new(weights).expect("Unreachable code");
        let mut rng = thread_rng();
        backends.get(dist.sample(&mut rng))
    }
}

struct SgGatewayInst {
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub routes: Vec<SgHttpRouteInst>,
    pub client: Client<HttpsConnector<HttpConnector>>,
    pub listeners: Vec<SgListener>,
}

#[derive(Default)]
struct SgHttpRouteInst {
    pub hostnames: Option<Vec<String>>,
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub rules: Option<Vec<SgHttpRouteRuleInst>>,
}

#[derive(Default)]
pub struct SgHttpRouteRuleInst {
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub matches: Option<Vec<SgHttpRouteMatchInst>>,
    pub backends: Option<Vec<SgBackendInst>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpRouteMatchInst {
    pub path: Option<SgHttpPathMatchInst>,
    pub header: Option<Vec<SgHttpHeaderMatchInst>>,
    pub query: Option<Vec<SgHttpQueryMatchInst>>,
    pub method: Option<Vec<String>>,
}
#[derive(Default, Debug, Clone)]

pub struct SgHttpPathMatchInst {
    pub kind: SgHttpPathMatchType,
    pub value: String,
    pub regular: Option<Regex>,
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpHeaderMatchInst {
    pub kind: SgHttpHeaderMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpQueryMatchInst {
    pub kind: SgHttpQueryMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

#[derive(Default)]
pub struct SgBackendInst {
    pub name_or_host: String,
    pub namespace: Option<String>,
    pub port: u16,
    pub timeout_ms: Option<u64>,
    pub protocol: Option<SgProtocol>,
    pub weight: Option<u16>,
    pub filters: Vec<(String, BoxSgPluginFilter)>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use http::{Method, Request};
    use hyper::Body;
    use tardis::regex::Regex;

    use crate::{
        config::http_route_dto::{SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType},
        functions::http_route::{
            choose_backend, match_route_insts_with_hostname_priority, SgBackendInst, SgHttpHeaderMatchInst, SgHttpQueryMatchInst, SgHttpRouteInst, SgHttpRouteRuleInst,
        },
    };

    use super::{match_route_process, match_rule_inst, SgHttpPathMatchInst, SgHttpRouteMatchInst};

    #[test]
    fn test_match_rule_inst() {
        // If there is no matching rule, the match is considered successful
        let (matched, matched_match_inst) = match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), None);
        assert!(matched);
        assert!(matched_match_inst.is_none());

        // -----------------------------------------------------
        // Match exact path
        let match_conds = vec![SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Exact,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        let (matched, matched_match_inst) = match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), Some(&match_conds));
        assert!(matched);
        assert!(matched_match_inst.is_some());

        // Match prefix path
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
                Some(&vec![SgHttpRouteMatchInst {
                    path: Some(SgHttpPathMatchInst {
                        kind: SgHttpPathMatchType::Prefix,
                        value: "/".to_string(),
                        regular: None,
                    }),
                    ..Default::default()
                }])
            )
            .0
        );

        let match_conds = vec![SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Prefix,
                value: "/iam".to_string(),
                regular: None,
            }),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/spi/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        let (matched, matched_match_inst) = match_rule_inst(
            &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
            Some(&match_conds),
        );
        assert!(matched);
        assert!(matched_match_inst.is_some());
        let (matched, matched_match_inst) = match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(), Some(&match_conds));
        assert!(matched);
        assert!(matched_match_inst.is_some());
        let (matched, matched_match_inst) = match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), Some(&match_conds));
        assert!(matched);
        assert!(matched_match_inst.is_some());

        // Match regular path
        let match_conds = vec![SgHttpRouteMatchInst {
            path: Some(SgHttpPathMatchInst {
                kind: SgHttpPathMatchType::Regular,
                value: "/iam/[0-9]+/hi".to_string(),
                regular: Some(Regex::new("/iam/[0-9]+/hi").unwrap()),
            }),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct/hi").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/001/hi/hi").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/001/hi/").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // -----------------------------------------------------
        // Match method
        let match_conds = vec![SgHttpRouteMatchInst {
            method: Some(vec!["get".to_string()]),
            ..Default::default()
        }];
        assert!(
            !match_rule_inst(
                &Request::builder().method("post").uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // -----------------------------------------------------
        // Match exact header
        let match_conds = vec![SgHttpRouteMatchInst {
            header: Some(vec![
                SgHttpHeaderMatchInst {
                    kind: SgHttpHeaderMatchType::Exact,
                    name: "X-Auth-User".to_string(),
                    value: "gdxr".to_string(),
                    regular: None,
                },
                SgHttpHeaderMatchInst {
                    kind: SgHttpHeaderMatchType::Exact,
                    name: "App".to_string(),
                    value: "a001".to_string(),
                    regular: None,
                },
            ]),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("Tenant", "t001").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "t001").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "a001").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("X-Auth-User", "gdxr").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "a001").header("X-auTh-User", "gdxr").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // Match regular header
        let match_conds = vec![SgHttpRouteMatchInst {
            header: Some(vec![SgHttpHeaderMatchInst {
                kind: SgHttpHeaderMatchType::Regular,
                name: "X-Id".to_string(),
                value: "^[0-9]+$".to_string(),
                regular: Some(Regex::new("^[0-9]+$").unwrap()),
            }]),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("x-iD", "t001").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("x-iD", "002").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // -----------------------------------------------------
        // Match exact query
        let match_conds = vec![SgHttpRouteMatchInst {
            query: Some(vec![
                SgHttpQueryMatchInst {
                    kind: SgHttpQueryMatchType::Exact,
                    name: "id".to_string(),
                    value: "gdxr".to_string(),
                    regular: None,
                },
                SgHttpQueryMatchInst {
                    kind: SgHttpQueryMatchType::Exact,
                    name: "name".to_string(),
                    value: "星航".to_string(),
                    regular: None,
                },
            ]),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=gdxr").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA%26id%3Dgdxr").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA%26id%3Dgdxr%26code%3D1").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // Match regular query
        let match_conds = vec![SgHttpRouteMatchInst {
            query: Some(vec![SgHttpQueryMatchInst {
                kind: SgHttpQueryMatchType::Regular,
                name: "id".to_string(),
                value: "id[a-z]+".to_string(),
                regular: Some(Regex::new("id[a-z]+").unwrap()),
            }]),
            ..Default::default()
        }];
        assert!(!match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), Some(&match_conds)).0);
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=gdxr").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=idAdef").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=idadef").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );

        // Match multiple
        let match_conds = vec![
            SgHttpRouteMatchInst {
                method: Some(vec!["put".to_string()]),
                query: Some(vec![SgHttpQueryMatchInst {
                    kind: SgHttpQueryMatchType::Regular,
                    name: "id".to_string(),
                    value: "id[a-z]+".to_string(),
                    regular: Some(Regex::new("id[a-z]+").unwrap()),
                }]),
                ..Default::default()
            },
            SgHttpRouteMatchInst {
                method: Some(vec!["post".to_string()]),
                ..Default::default()
            },
        ];
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=idAdef").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            !match_rule_inst(
                &Request::builder().uri("https://sg.idealworld.group/?id=idadef").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().method(Method::from_bytes("put".as_bytes()).unwrap()).uri("https://sg.idealworld.group/?id=idadef").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
        assert!(
            match_rule_inst(
                &Request::builder().method(Method::from_bytes("post".as_bytes()).unwrap()).uri("https://any").body(Body::empty()).unwrap(),
                Some(&match_conds)
            )
            .0
        );
    }

    #[test]
    fn test_match_route_insts_with_hostname_priority() {
        // Match all hostname
        assert!(!match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: None,
                ..Default::default()
            }]
        )
        .2
        .is_empty());
        assert!(!match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*".to_string()]),
                ..Default::default()
            }]
        )
        .2
        .is_empty());
        assert!(!match_route_insts_with_hostname_priority(
            None,
            &vec![SgHttpRouteInst {
                hostnames: None,
                ..Default::default()
            }]
        )
        .2
        .is_empty());
        // Match exact hostname
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .0
        .is_empty());
        assert!(!match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .0
        .is_empty());
        // Match wildcard hostname
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .1
        .is_empty());
        assert!(!match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .1
        .is_empty());
        assert!(!match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.*".to_string()]),
                ..Default::default()
            }]
        )
        .1
        .is_empty());

        // Match with priority
        let match_conds = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string(), "spacegate.idealworld.group".to_string()]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["www.idealworld.group".to_string(), "idealworld.group".to_string()]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["*".to_string()]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: None,
                ..Default::default()
            },
        ];
        let (highest, second, lowest) = match_route_insts_with_hostname_priority(Some("sg.idealworld.com"), &match_conds);
        assert!(highest.is_empty() && second.is_empty() && lowest.len() == 2);
        let (highest, second, lowest) = match_route_insts_with_hostname_priority(Some("sg.idealworld.group"), &match_conds);
        assert!(
            highest.len() == 1
                && highest.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"sg.idealworld.group".to_string()))
                && second.len() == 1
                && second.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"*.idealworld.group".to_string()))
                && lowest.len() == 2
        );
        let (highest, second, lowest) = match_route_insts_with_hostname_priority(Some("spacegate.idealworld.group"), &match_conds);
        assert!(
            highest.len() == 1
                && highest.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"spacegate.idealworld.group".to_string()))
                && second.len() == 1
                && second.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"*.idealworld.group".to_string()))
                && lowest.len() == 2
        );
        let (highest, second, lowest) = match_route_insts_with_hostname_priority(Some("api.idealworld.group"), &match_conds);
        assert!(
            highest.is_empty()
                && second.len() == 1
                && second.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"*.idealworld.group".to_string()))
                && lowest.len() == 2
        );
        let (highest, second, lowest) = match_route_insts_with_hostname_priority(Some("idealworld.group"), &match_conds);
        assert!(
            highest.len() == 1 && highest.iter().any(|route| route.hostnames.as_ref().unwrap().contains(&"idealworld.group".to_string())) && second.is_empty() && lowest.len() == 2
        );
    }

    #[test]
    fn test_match_route_process() {
        // Match all hostname
        let test_routes = vec![SgHttpRouteInst {
            hostnames: None,
            ..Default::default()
        }];
        let (matched_route, matched_rule, matched_match) = match_route_process(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_none() && matched_match.is_none());

        let test_routes = vec![SgHttpRouteInst {
            hostnames: Some(vec!["sg.idealworld.group".to_string()]),
            ..Default::default()
        }];
        let (matched_route, matched_rule, matched_match) = match_route_process(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_none() && matched_match.is_none());

        let test_routes = vec![SgHttpRouteInst {
            hostnames: None,
            rules: Some(vec![SgHttpRouteRuleInst { ..Default::default() }]),
            ..Default::default()
        }];
        let (matched_route, matched_rule, matched_match) = match_route_process(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_none());

        let test_routes = vec![SgHttpRouteInst {
            hostnames: None,
            rules: Some(vec![
                SgHttpRouteRuleInst { ..Default::default() },
                SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Exact,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        }];
        let (matched_route, matched_rule, matched_match) =
            match_route_process(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_some());

        let test_routes = vec![SgHttpRouteInst {
            hostnames: None,
            rules: Some(vec![SgHttpRouteRuleInst {
                matches: Some(vec![SgHttpRouteMatchInst {
                    path: Some(SgHttpPathMatchInst {
                        kind: SgHttpPathMatchType::Prefix,
                        value: "/".to_string(),
                        regular: None,
                    }),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }];
        let (matched_route, matched_rule, matched_match) = match_route_process(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_some());

        //Multiple route matching test
        let test_routes = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst { ..Default::default() }]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst { ..Default::default() }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Exact,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        ];
        let (matched_route, matched_rule, matched_match) =
            match_route_process(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_some());

        let test_routes = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: None,
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Exact,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        ];
        let (matched_route, matched_rule, matched_match) =
            match_route_process(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_some());

        // Although the second match is more consistent, the order of matching is still the first one to be matched
        let test_routes = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Prefix,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Prefix,
                            value: "/iam/a".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        ];
        let (matched_route, matched_rule, matched_match) =
            match_route_process(&Request::builder().uri("https://sg.idealworld.group/iam/a").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_some() && matched_match.is_some());
        assert!(matched_match.unwrap().path.as_ref().unwrap().value == "/iam");

        let test_routes = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: None,
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Exact,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        ];
        let (matched_route, matched_rule, matched_match) =
            match_route_process(&Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(), &test_routes);
        assert!(matched_route.is_some() && matched_rule.is_none() && matched_match.is_none());

        //mix match test
        let _test_routes = vec![
            SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                rules: None,
                ..Default::default()
            },
            SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                rules: Some(vec![SgHttpRouteRuleInst {
                    matches: Some(vec![SgHttpRouteMatchInst {
                        path: Some(SgHttpPathMatchInst {
                            kind: SgHttpPathMatchType::Exact,
                            value: "/iam".to_string(),
                            regular: None,
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        ];
    }

    #[test]
    fn test_choose_backend() {
        // Only one backend
        assert!(
            choose_backend(&vec![SgBackendInst {
                name_or_host: "iam1".to_string(),
                weight: None,
                ..Default::default()
            }])
            .unwrap()
            .name_or_host
                == "iam1"
        );

        // Check weight
        let backends = vec![
            SgBackendInst {
                name_or_host: "iam1".to_string(),
                weight: Some(30),
                ..Default::default()
            },
            SgBackendInst {
                name_or_host: "iam2".to_string(),
                weight: Some(70),
                ..Default::default()
            },
        ];
        let mut backend_counts = HashMap::new();
        for _ in 0..1000 {
            let backend = choose_backend(&backends);
            *backend_counts.entry(backend.as_ref().unwrap().name_or_host.clone()).or_insert(0) += 1;
        }
        println!("backend_counts: {:?}", backend_counts);
        assert!(backend_counts.get("iam1").unwrap() < backend_counts.get("iam2").unwrap());
    }
}
