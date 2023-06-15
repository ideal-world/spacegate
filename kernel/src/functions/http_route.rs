use std::{collections::HashMap, net::SocketAddr};

use crate::{
    config::{
        gateway_dto::{SgGateway, SgListener, SgProtocol},
        http_route_dto::{SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType, SgHttpRoute},
    },
    plugins::{
        context::{ChoseHttpRouteRuleInst, SgRouteFilterContext, SgRouteFilterRequestAction},
        filters::{self, BoxSgPluginFilter},
    },
};
use http::{header::UPGRADE, uri::Scheme, Request, Response};
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
    let all_route_rules = routes.iter().flat_map(|route| route.rules.clone().unwrap_or_default()).collect::<Vec<_>>();
    let global_filters = if let Some(filters) = gateway_conf.filters {
        filters::init(filters, &all_route_rules).await?
    } else {
        Vec::new()
    };
    let mut route_insts = Vec::new();
    for route in routes.clone() {
        let route_filters = if let Some(filters) = route.filters {
            filters::init(filters, &route.rules.clone().unwrap_or_default()).await?
        } else {
            Vec::new()
        };
        let rule_insts = if let Some(rules) = route.rules {
            let mut rule_insts = Vec::new();
            for rule in rules {
                let rule_filters = if let Some(filters) = rule.filters.clone() {
                    filters::init(filters, &[rule.clone()]).await?
                } else {
                    Vec::new()
                };

                let rule_matches_insts = rule
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

                rule_insts.push(SgHttpRouteRuleInst {
                    filters: rule_filters,
                    matches: rule_matches_insts,
                    backends: if let Some(backend_refs) = rule.backends {
                        let backends = join_all(
                            backend_refs
                                .into_iter()
                                .map(|backend_ref| (backend_ref, &all_route_rules))
                                .map(|(backend_ref, read_only_routes)| async move {
                                    let filters = if let Some(filters) = backend_ref.filters {
                                        filters::init(filters, read_only_routes).await?
                                    } else {
                                        Vec::new()
                                    };
                                    Ok::<_, TardisError>(SgBackend {
                                        name_or_host: backend_ref.name_or_host,
                                        namespace: backend_ref.namespace,
                                        port: backend_ref.port,
                                        timeout_ms: backend_ref.timeout_ms,
                                        protocol: backend_ref.protocol,
                                        weight: backend_ref.weight,
                                        filters,
                                    })
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
    }
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

pub async fn process(gateway_name: Arc<String>, req_scheme: &str, remote_addr: SocketAddr, mut request: Request<Body>) -> TardisResult<Response<Body>> {
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
        "[SG.Route] Request method {} url {} , from {} @ {}",
        request.method(),
        request.uri(),
        remote_addr,
        gateway_name
    );
    let gateway_inst = get(&gateway_name)?;
    if !match_listeners_hostname_and_port(
        request.uri().host(),
        request.uri().port().map_or_else(
            || {
                if request.uri().scheme().unwrap_or(&Scheme::HTTP) == &Scheme::HTTPS {
                    443
                } else {
                    80
                }
            },
            |p| p.as_u16(),
        ),
        &gateway_inst.listeners,
    ) {
        log::trace!("[SG.Route] Request hostname {} not match", request.uri().host().expect(""));
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        return Ok(not_found);
    }

    let matched_route_inst = match_route_insts_with_hostname_priority(request.uri().host(), &gateway_inst.routes);
    if matched_route_inst.is_none() {
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        return Ok(not_found);
    }

    let matched_route_inst = matched_route_inst.expect("Unreachable code");
    let (matched_rule_inst, matched_match_inst) = if let Some(matched_rule_insts) = &matched_route_inst.rules {
        let mut _matched_rule_inst = None;
        let mut _matched_match_inst = None;
        for matched_rule_inst in matched_rule_insts {
            let (matched, matched_match_inst) = match_rule_inst(&request, matched_rule_inst.matches.as_ref());
            if !matched {
                continue;
            }
            _matched_rule_inst = Some(matched_rule_inst);
            _matched_match_inst = matched_match_inst;
        }
        (_matched_rule_inst, _matched_match_inst)
    } else {
        (None, None)
    };

    let backend = if let Some(Some(backends)) = matched_rule_inst.map(|rule| &rule.backends) {
        choose_backend(backends)
    } else {
        None
    };

    if request.headers().get(UPGRADE).map(|v| &v.to_str().expect("[SG.Websocket] Upgrade header value illegal:  is not ascii").to_lowercase() == "websocket").unwrap_or(false) {
        #[cfg(feature = "ws")]
        {
            if let Some(backend) = backend {
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

    let backend_filters = backend.map(|backend| &backend.filters);
    let rule_filters = matched_rule_inst.map(|rule| &rule.filters);

    let ctx = process_req_filters(
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
        http_client::request(&gateway_inst.client, backend, rule_timeout, ctx.get_action() == &SgRouteFilterRequestAction::Redirect, ctx).await?
    };

    let mut ctx: SgRouteFilterContext = process_resp_filters(ctx, backend_filters, rule_filters, &matched_route_inst.filters, &gateway_inst.filters, matched_match_inst).await?;

    ctx.build_response().await
}

fn match_route_insts_with_hostname_priority<'a>(req_host: Option<&str>, routes: &'a Vec<SgHttpRouteInst>) -> Option<&'a SgHttpRouteInst> {
    if let Some(req_host) = req_host {
        let mut matched_route_by_wildcard: Option<&SgHttpRouteInst> = None;
        let mut matched_route_by_no_set = None;
        for route_inst in routes {
            if let Some(hostnames) = &route_inst.hostnames {
                if hostnames.iter().any(|hostname| hostname == req_host) {
                    // Exact match, highest priority
                    return Some(route_inst);
                } else if matched_route_by_wildcard.is_none() {
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
                        matched_route_by_wildcard = Some(route_inst);
                    }
                }
            } else if matched_route_by_no_set.is_none() {
                // No Hostname specified, equal to *, lowest priority
                matched_route_by_no_set = Some(route_inst)
            }
        }
        if matched_route_by_wildcard.is_some() {
            matched_route_by_wildcard
        } else {
            matched_route_by_no_set
        }
    } else {
        routes.iter().find(|route_inst| route_inst.hostnames.is_none())
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

async fn process_req_filters(
    gateway_name: String,
    remote_addr: SocketAddr,
    request: Request<Body>,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_rule_inst: Option<&SgHttpRouteRuleInst>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRouteFilterContext> {
    let mut ctx = SgRouteFilterContext::new(
        request.method().clone(),
        request.uri().clone(),
        request.version(),
        request.headers().clone(),
        request.into_body(),
        remote_addr,
        gateway_name,
        matched_rule_inst.map(|m| ChoseHttpRouteRuleInst::clone_from(m, matched_match_inst)),
    );
    let mut is_continue;
    let mut executed_filters = Vec::new();
    if let Some(backend_filters) = backend_filters {
        for (id, filter) in backend_filters {
            if !executed_filters.contains(&id) {
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
            if !executed_filters.contains(&id) {
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
        if !executed_filters.contains(&id) {
            log::trace!("[SG.Plugin.Filter] Hit id {id} in request");
            (is_continue, ctx) = filter.req_filter(id, ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(id);
        }
    }
    for (id, filter) in global_filters {
        if !executed_filters.contains(&id) {
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
    mut ctx: SgRouteFilterContext,
    backend_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    rule_filters: Option<&Vec<(String, BoxSgPluginFilter)>>,
    route_filters: &Vec<(String, BoxSgPluginFilter)>,
    global_filters: &Vec<(String, BoxSgPluginFilter)>,
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRouteFilterContext> {
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

fn choose_backend(backends: &Vec<SgBackend>) -> Option<&SgBackend> {
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
    pub backends: Option<Vec<SgBackend>>,
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
pub struct SgBackend {
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
        functions::http_route::{choose_backend, match_route_insts_with_hostname_priority, SgBackend, SgHttpHeaderMatchInst, SgHttpQueryMatchInst, SgHttpRouteInst},
    };

    use super::{match_rule_inst, SgHttpPathMatchInst, SgHttpRouteMatchInst};

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
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: None,
                ..Default::default()
            }]
        )
        .is_some());
        assert!(match_route_insts_with_hostname_priority(
            None,
            &vec![SgHttpRouteInst {
                hostnames: None,
                ..Default::default()
            }]
        )
        .is_some());
        // Match exact hostname
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .is_none());
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["sg.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .is_some());
        // Match wildcard hostname
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.com"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .is_none());
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.group".to_string()]),
                ..Default::default()
            }]
        )
        .is_some());
        assert!(match_route_insts_with_hostname_priority(
            Some("sg.idealworld.group"),
            &vec![SgHttpRouteInst {
                hostnames: Some(vec!["*.idealworld.*".to_string()]),
                ..Default::default()
            }]
        )
        .is_some());

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
        ];
        assert!(match_route_insts_with_hostname_priority(Some("sg.idealworld.com"), &match_conds).is_none());
        assert!(match_route_insts_with_hostname_priority(Some("sg.idealworld.group"), &match_conds)
            .unwrap()
            .hostnames
            .as_ref()
            .unwrap()
            .contains(&"sg.idealworld.group".to_string()));
        assert!(match_route_insts_with_hostname_priority(Some("spacegate.idealworld.group"), &match_conds)
            .unwrap()
            .hostnames
            .as_ref()
            .unwrap()
            .contains(&"sg.idealworld.group".to_string()));
        assert!(match_route_insts_with_hostname_priority(Some("api.idealworld.group"), &match_conds)
            .unwrap()
            .hostnames
            .as_ref()
            .unwrap()
            .contains(&"*.idealworld.group".to_string()));
        assert!(match_route_insts_with_hostname_priority(Some("idealworld.group"), &match_conds).unwrap().hostnames.as_ref().unwrap().contains(&"idealworld.group".to_string()));
    }

    #[test]
    fn test_choose_backend() {
        // Only one backend
        assert!(
            choose_backend(&vec![SgBackend {
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
            SgBackend {
                name_or_host: "iam1".to_string(),
                weight: Some(30),
                ..Default::default()
            },
            SgBackend {
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
