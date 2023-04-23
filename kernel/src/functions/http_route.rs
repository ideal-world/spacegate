use std::{collections::HashMap, net::SocketAddr};

use crate::{
    config::{
        gateway_dto::{SgGateway, SgProtocol},
        http_route_dto::{SgHttpBackendRef, SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryParamMatchType, SgHttpRoute},
    },
    plugins::filters::{self, SgPluginFilter, SgRouteFilterContext, SgRouteFilterRequestAction},
};
use http::{HeaderValue, Request, Response};
use hyper::{Body, Client, StatusCode};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::vec::Vec;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    regex::Regex,
    TardisFuns,
};

static mut ROUTES: Option<HashMap<String, SgGatewayInst>> = None;

pub async fn init(gateway_conf: SgGateway, routes: Vec<SgHttpRoute>) -> TardisResult<()> {
    let global_filters = if let Some(filters) = gateway_conf.filters {
        filters::init(filters).await?
    } else {
        Vec::new()
    };
    let mut route_insts = Vec::new();
    for route in routes {
        let route_filters = if let Some(filters) = route.filters { filters::init(filters).await? } else { Vec::new() };
        let rule_insts = if let Some(rules) = route.rules {
            let mut rule_insts = Vec::new();
            for rule in rules {
                let rule_filters = if let Some(filters) = rule.filters { filters::init(filters).await? } else { Vec::new() };

                let rule_matches_insts = rule.matches.map(|rule_matches| {
                    rule_matches
                        .into_iter()
                        .map(|rule_match| {
                            let path_inst = rule_match.path.map(|path| SgHttpPathMatchInst {
                                regular: if path.kind == SgHttpPathMatchType::Regular {
                                    Some(
                                        Regex::new(&path.value)
                                            .map_err(|_| TardisError::format_error(&format!("[SG.Route] Path Regular {} format error", path.value), ""))
                                            .unwrap(),
                                    )
                                } else {
                                    None
                                },
                                kind: path.kind,
                                value: path.value,
                            });

                            let header_inst = rule_match.header.map(|header| {
                                header
                                    .into_iter()
                                    .map(|header| SgHttpHeaderMatchInst {
                                        regular: if header.kind == SgHttpHeaderMatchType::Regular {
                                            Some(
                                                Regex::new(&header.value)
                                                    .map_err(|_| TardisError::format_error(&format!("[SG.Route] Header Regular {} format error", header.value), ""))
                                                    .unwrap(),
                                            )
                                        } else {
                                            None
                                        },
                                        kind: header.kind,
                                        name: header.name.clone(),
                                        value: header.value.clone(),
                                    })
                                    .collect()
                            });

                            let query_inst = rule_match.query.map(|query| {
                                query
                                    .into_iter()
                                    .map(|query| SgHttpQueryParamMatchInst {
                                        regular: if query.kind == SgHttpQueryParamMatchType::Regular {
                                            Some(
                                                Regex::new(&query.value)
                                                    .map_err(|_| TardisError::format_error(&format!("[SG.Route] Query Regular {} format error", query.value), ""))
                                                    .unwrap(),
                                            )
                                        } else {
                                            None
                                        },
                                        kind: query.kind,
                                        name: query.name.clone(),
                                        value: query.value.clone(),
                                    })
                                    .collect()
                            });

                            SgHttpRouteMatchInst {
                                path: path_inst,
                                header: header_inst,
                                query: query_inst,
                                method: rule_match.method.map(|m| m.to_uppercase()),
                            }
                        })
                        .collect_vec()
                });

                rule_insts.push(SgHttpRouteRuleInst {
                    filters: rule_filters,
                    matches: rule_matches_insts,
                    backends: rule.backends,
                })
            }
            Some(rule_insts)
        } else {
            None
        };
        route_insts.push(SgHttpRouteInst {
            hostnames: route.hostnames.map(|hostnames| hostnames.into_iter().map(|hostname| hostname.to_lowercase()).collect_vec()),
            filters: route_filters,
            rules: rule_insts,
        })
    }
    let route_inst = SgGatewayInst {
        filters: global_filters,
        routes: route_insts,
        client: Client::new(),
    };
    unsafe {
        if ROUTES.is_none() {
            ROUTES = Some(HashMap::new());
        }
        ROUTES.as_mut().unwrap().insert(gateway_conf.name.to_string(), route_inst);
    }
    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    let route = unsafe {
        if ROUTES.is_none() {
            ROUTES = Some(HashMap::new());
        }
        ROUTES.as_mut().unwrap().remove(name)
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
        if let Some(routes) = ROUTES.as_ref().unwrap().get(name) {
            Ok(routes)
        } else {
            Err(TardisError::bad_request(&format!("[SG.server] Get routes {name} failed"), ""))
        }
    }
}

pub async fn process(gateway_name: Arc<String>, remote_addr: SocketAddr, request: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let gateway_inst = match get(&gateway_name) {
        Ok(route) => route,
        Err(error) => return process_error(error),
    };

    let matched_route_inst = match_route_insts_with_priority(&request, gateway_inst);
    if let Some(matched_route_inst) = matched_route_inst {
        if let Some(Some(matched_rule_inst)) = matched_route_inst.rules.map(|rules| rules.iter().find(|rule| match_rule_inst(&request, rule))) {
            let ctx = process_req_filters(remote_addr, request, Some(&matched_rule_inst.filters), &matched_route_inst.filters, &gateway_inst.filters).await?;
            let response = if let Some(backends) = &matched_rule_inst.backends {
                let backend = choose_backend(backends);
                do_request(&gateway_inst.client, backend, ctx).await?;
            } else {
                do_request(&gateway_inst.client, None, ctx).await?;
            };
        } else {
            let ctx = process_req_filters(remote_addr, request, None, &matched_route_inst.filters, &gateway_inst.filters).await?;
            let response = do_request(&gateway_inst.client, None, ctx).await?;
        }
    } else {
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        Ok(not_found)
    }
}

async fn do_request(client: &Client<Body>, backend: Option<SgHttpBackendRef>, ctx: SgRouteFilterContext) -> TardisResult<()> {
    let mut req = Request::builder();
    req = req.method(ctx.method);
    if let Some(backend) = backend {
        let url = format!(
            "{}://{}:{}/{}",
            backend.protocol.as_ref().unwrap_or(&SgProtocol::Http),
            backend.namespace_or_host.as_ref().unwrap_or(&"default".to_string()),
            backend.port,
            backend.name_or_path
        );
        req = req.uri(url);
    } else {
        req = req.uri(ctx.uri);
    }
    for (k, v) in ctx.req_headers {
        req.header(k.unwrap().as_str(), v.to_str().unwrap());
    }
    req.body(ctx.get_raw_body());
    let response = client.request(req).await;

    Ok(())
}

fn choose_backend(backends: &Vec<SgHttpBackendRef>) -> Option<&SgHttpBackendRef> {
    // TODO
    backends.get(0)
}

fn match_route_insts_with_priority<'a>(req: &Request<Body>, gateway_inst: &'a SgGatewayInst) -> Option<&'a SgHttpRouteInst> {
    if let Some(req_host) = req.uri().host() {
        let mut matched_route_by_wildcard: Option<&SgHttpRouteInst> = None;
        let mut matched_route_by_no_set = None;
        for route_inst in &gateway_inst.routes {
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
        None
    }
}

fn match_rule_inst(req: &Request<Body>, rule_inst: &SgHttpRouteRuleInst) -> bool {
    if let Some(matches) = &rule_inst.matches {
        for rule_match in matches {
            if let Some(method) = &rule_match.method {
                if req.method().as_str() != method {
                    return false;
                }
            }
            if let Some(path) = &rule_match.path {
                let req_path = req.uri().path();
                match path.kind {
                    SgHttpPathMatchType::Exact => {
                        if req_path != &path.value {
                            return false;
                        }
                    }
                    SgHttpPathMatchType::Prefix => {
                        if !req_path.starts_with(&path.value) {
                            return false;
                        }
                    }
                    SgHttpPathMatchType::Regular => {
                        if !&path.regular.as_ref().unwrap().is_match(req_path) {
                            return false;
                        }
                    }
                }
            }
            if let Some(headers) = &rule_match.header {
                for header in headers {
                    if let Some(req_header_value) = req.headers().get(&header.name) {
                        if req_header_value.is_empty() {
                            return false;
                        }
                        let req_header_value = req_header_value.to_str();
                        if req_header_value.is_err() {
                            return false;
                        }
                        let req_header_value = req_header_value.unwrap();
                        match header.kind {
                            SgHttpHeaderMatchType::Exact => {
                                if req_header_value != &header.value {
                                    return false;
                                }
                            }
                            SgHttpHeaderMatchType::Regular => {
                                if !&header.regular.as_ref().unwrap().is_match(req_header_value) {
                                    return false;
                                }
                            }
                        }
                    } else {
                        return false;
                    }
                }
            }
            if let Some(queries) = &rule_match.query {
                for query in queries {
                    if let Some(Some(req_query_value)) = req.uri().query().map(|q| {
                        q.split('&')
                            .collect::<Vec<&str>>()
                            .into_iter()
                            .map(|item| {
                                let mut item = item.split('=');
                                if item.next().is_some() {
                                    (item.next().unwrap(), item.next().unwrap_or(""))
                                } else {
                                    ("", "")
                                }
                            })
                            .find(|(k, _)| k == &query.name)
                            .map(|(_, v)| v)
                    }) {
                        match query.kind {
                            SgHttpQueryParamMatchType::Exact => {
                                if req_query_value != &query.value {
                                    return false;
                                }
                            }
                            SgHttpQueryParamMatchType::Regular => {
                                if !&query.regular.as_ref().unwrap().is_match(&req_query_value) {
                                    return false;
                                }
                            }
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    } else {
        true
    }
}

async fn process_req_filters(
    remote_addr: SocketAddr,
    request: Request<Body>,
    rule_filters: Option<&Vec<(String, Box<dyn SgPluginFilter>)>>,
    route_filers: &Vec<(String, Box<dyn SgPluginFilter>)>,
    global_filters: &Vec<(String, Box<dyn SgPluginFilter>)>,
) -> TardisResult<SgRouteFilterContext> {
    let mut ctx = SgRouteFilterContext {
        method: request.method().clone(),
        uri: request.uri().clone(),
        version: request.version(),
        req_headers: request.headers().clone(),
        remote_addr: remote_addr,
        resp_headers: None,
        resp_status_code: None,
        ext: HashMap::new(),
        inner_body: None,
        raw_body: Some(request.into_body()),
        action: SgRouteFilterRequestAction::None,
    };
    let mut is_continue = true;
    let mut executed_filters = Vec::new();
    if let Some(rule_filters) = rule_filters {
        for (name, filter) in rule_filters {
            if !executed_filters.contains(&name) {
                (is_continue, ctx) = filter.req_filter(ctx).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(name);
            }
        }
    }
    for (name, filter) in route_filers {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.req_filter(ctx).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    for (name, filter) in global_filters {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.req_filter(ctx).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    Ok(ctx)
}

async fn process_resp_filters(
    mut ctx: SgRouteFilterContext,
    rule_filters: Option<&Vec<(String, Box<dyn SgPluginFilter>)>>,
    route_filers: &Vec<(String, Box<dyn SgPluginFilter>)>,
    global_filters: &Vec<(String, Box<dyn SgPluginFilter>)>,
) -> TardisResult<SgRouteFilterContext> {
    let mut is_continue = true;
    let mut executed_filters = Vec::new();
    if let Some(rule_filters) = rule_filters {
        for (name, filter) in rule_filters {
            if !executed_filters.contains(&name) {
                (is_continue, ctx) = filter.resp_filter(ctx).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(name);
            }
        }
    }
    for (name, filter) in route_filers {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.resp_filter(ctx).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    for (name, filter) in global_filters {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.resp_filter(ctx).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    Ok(ctx)
}

pub fn process_error(error: TardisError) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::new(Body::from(
        TardisFuns::json
            .obj_to_string(&SgRespError {
                code: error.code,
                msg: error.message,
            })
            .unwrap(),
    ));
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
    Ok(response)
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct SgRespError {
    pub code: String,
    pub msg: String,
}

struct SgGatewayInst {
    pub filters: Vec<(String, Box<dyn SgPluginFilter>)>,
    pub routes: Vec<SgHttpRouteInst>,
    pub client: Client<Body>,
}

struct SgHttpRouteInst {
    pub hostnames: Option<Vec<String>>,
    pub filters: Vec<(String, Box<dyn SgPluginFilter>)>,
    pub rules: Option<Vec<SgHttpRouteRuleInst>>,
}

struct SgHttpRouteRuleInst {
    pub filters: Vec<(String, Box<dyn SgPluginFilter>)>,
    pub matches: Option<Vec<SgHttpRouteMatchInst>>,
    pub backends: Option<Vec<SgHttpBackendRef>>,
}

pub struct SgHttpRouteMatchInst {
    pub path: Option<SgHttpPathMatchInst>,
    pub header: Option<Vec<SgHttpHeaderMatchInst>>,
    pub query: Option<Vec<SgHttpQueryParamMatchInst>>,
    pub method: Option<String>,
}

pub struct SgHttpPathMatchInst {
    pub kind: SgHttpPathMatchType,
    pub value: String,
    pub regular: Option<Regex>,
}

pub struct SgHttpHeaderMatchInst {
    pub kind: SgHttpHeaderMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

pub struct SgHttpQueryParamMatchInst {
    pub kind: SgHttpQueryParamMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}
