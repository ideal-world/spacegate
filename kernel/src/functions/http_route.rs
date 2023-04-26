use std::{collections::HashMap, net::SocketAddr};

use crate::{
    config::{
        gateway_dto::SgGateway,
        http_route_dto::{SgHttpBackendRef, SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType, SgHttpRoute},
    },
    plugins::filters::{self, SgPluginFilter, SgRouteFilterContext, SgRouteFilterRequestAction},
};
use http::{HeaderValue, Request, Response};
use hyper::{client::HttpConnector, Body, Client, StatusCode};
use hyper_rustls::HttpsConnector;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::vec::Vec;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    log,
    rand::{distributions::WeightedIndex, prelude::Distribution, thread_rng},
    regex::Regex,
    TardisFuns,
};

use super::client;

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
                                        value: header.value,
                                    })
                                    .collect()
                            });

                            let query_inst = rule_match.query.map(|query| {
                                query
                                    .into_iter()
                                    .map(|query| SgHttpQueryMatchInst {
                                        regular: if query.kind == SgHttpQueryMatchType::Regular {
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
                                        value: query.value,
                                    })
                                    .collect()
                            });

                            SgHttpRouteMatchInst {
                                path: path_inst,
                                header: header_inst,
                                query: query_inst,
                                method: rule_match.method.map(|m| m.into_iter().map(|m| m.to_uppercase()).collect_vec()),
                            }
                        })
                        .collect_vec()
                });

                rule_insts.push(SgHttpRouteRuleInst {
                    filters: rule_filters,
                    matches: rule_matches_insts,
                    backends: rule.backends,
                    timeout: rule.timeout,
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
        client: client::init()?,
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
            Err(TardisError::bad_request(&format!("[SG.Route] Get routes {name} failed"), ""))
        }
    }
}

pub async fn process(gateway_name: Arc<String>, req_scheme: &str, remote_addr: SocketAddr, request: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match do_process(gateway_name, req_scheme, remote_addr, request).await {
        Ok(result) => Ok(result),
        Err(error) => into_http_error(error),
    }
}

async fn do_process(gateway_name: Arc<String>, req_scheme: &str, remote_addr: SocketAddr, mut request: Request<Body>) -> TardisResult<Response<Body>> {
    if request.uri().host().is_none() && request.headers().contains_key("Host") {
        *request.uri_mut() = format!("{}://{}{}", req_scheme, request.headers().get("Host").unwrap().to_str().unwrap(), request.uri()).parse().unwrap();
    }
    log::trace!(
        "[SG.Route] Request method {} url {} , from {} @ {}",
        request.method(),
        request.uri(),
        remote_addr,
        gateway_name
    );
    let gateway_inst = get(&gateway_name)?;

    let matched_route_inst = match_route_insts_with_hostname_priority(request.uri().host(), &gateway_inst.routes);
    if matched_route_inst.is_none() {
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        return Ok(not_found);
    }

    let matched_route_inst = matched_route_inst.unwrap();
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
    let rule_filters = matched_rule_inst.map(|rule| &rule.filters);

    let ctx = process_req_filters(
        gateway_name.to_string(),
        remote_addr,
        request,
        rule_filters,
        &matched_route_inst.filters,
        &gateway_inst.filters,
        matched_match_inst,
    )
    .await?;

    let rule_timeout = if let Some(timeout) = matched_rule_inst.map(|rule| rule.timeout) {
        timeout
    } else {
        None
    };

    let ctx = if ctx.get_action() == &SgRouteFilterRequestAction::Response {
        ctx
    } else if let Some(Some(backends)) = matched_rule_inst.map(|rule| &rule.backends) {
        let backend = choose_backend(backends);
        client::request(&gateway_inst.client, backend, rule_timeout, ctx.get_action() == &SgRouteFilterRequestAction::Redirect, ctx).await?
    } else {
        client::request(&gateway_inst.client, None, rule_timeout, ctx.get_action() == &SgRouteFilterRequestAction::Redirect, ctx).await?
    };

    let mut ctx = process_resp_filters(ctx, rule_filters, &matched_route_inst.filters, &gateway_inst.filters, matched_match_inst).await?;

    let mut resp = Response::builder();
    for (k, v) in ctx.get_resp_headers() {
        resp = resp.header(k.as_str(), v.to_str().unwrap());
    }
    let resp =
        resp.body(ctx.pop_resp_body_raw()?.unwrap_or_else(Body::empty)).map_err(|error| TardisError::internal_error(&format!("[SG.Route] Build response error:{error}"), ""))?;
    Ok(resp)
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
                        if req_path != &path.value {
                            continue;
                        }
                    }
                    SgHttpPathMatchType::Prefix => {
                        if !req_path.starts_with(&path.value) {
                            continue;
                        }
                    }
                    SgHttpPathMatchType::Regular => {
                        if !&path.regular.as_ref().unwrap().is_match(req_path) {
                            continue;
                        }
                    }
                }
            }
            if let Some(headers) = &rule_match.header {
                for header in headers {
                    if let Some(req_header_value) = req.headers().get(&header.name) {
                        if req_header_value.is_empty() {
                            continue;
                        }
                        let req_header_value = req_header_value.to_str();
                        if req_header_value.is_err() {
                            continue;
                        }
                        let req_header_value = req_header_value.unwrap();
                        match header.kind {
                            SgHttpHeaderMatchType::Exact => {
                                if req_header_value != &header.value {
                                    continue;
                                }
                            }
                            SgHttpHeaderMatchType::Regular => {
                                if !&header.regular.as_ref().unwrap().is_match(req_header_value) {
                                    continue;
                                }
                            }
                        }
                    } else {
                        continue;
                    }
                }
            }
            if let Some(queries) = &rule_match.query {
                for query in queries {
                    if let Some(Some(req_query_value)) = req.uri().query().map(|q| {
                        let q = urlencoding::decode(q).unwrap();
                        let q = q.as_ref().split('&').collect_vec();
                        q.into_iter().map(|item| item.split('=').collect_vec()).find(|item| item.len() == 2 && item[0] == &query.name).map(|item| item[1].to_string())
                    }) {
                        match query.kind {
                            SgHttpQueryMatchType::Exact => {
                                if req_query_value != query.value {
                                    continue;
                                }
                            }
                            SgHttpQueryMatchType::Regular => {
                                if !&query.regular.as_ref().unwrap().is_match(&req_query_value) {
                                    continue;
                                }
                            }
                        }
                    } else {
                        continue;
                    }
                }
            }
            return (true, Some(rule_match));
        }
        (false, None)
    } else {
        (true, None)
    }
}

async fn process_req_filters(
    gateway_name: String,
    remote_addr: SocketAddr,
    request: Request<Body>,
    rule_filters: Option<&Vec<(String, Box<dyn SgPluginFilter>)>>,
    route_filers: &Vec<(String, Box<dyn SgPluginFilter>)>,
    global_filters: &Vec<(String, Box<dyn SgPluginFilter>)>,
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
    );
    let mut is_continue;
    let mut executed_filters = Vec::new();
    if let Some(rule_filters) = rule_filters {
        for (name, filter) in rule_filters {
            if !executed_filters.contains(&name) {
                (is_continue, ctx) = filter.req_filter(ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(name);
            }
        }
    }
    for (name, filter) in route_filers {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.req_filter(ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    for (name, filter) in global_filters {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.req_filter(ctx, matched_match_inst).await?;
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
    matched_match_inst: Option<&SgHttpRouteMatchInst>,
) -> TardisResult<SgRouteFilterContext> {
    let mut is_continue;
    let mut executed_filters = Vec::new();
    if let Some(rule_filters) = rule_filters {
        for (name, filter) in rule_filters {
            if !executed_filters.contains(&name) {
                (is_continue, ctx) = filter.resp_filter(ctx, matched_match_inst).await?;
                if !is_continue {
                    return Ok(ctx);
                }
                executed_filters.push(name);
            }
        }
    }
    for (name, filter) in route_filers {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.resp_filter(ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    for (name, filter) in global_filters {
        if !executed_filters.contains(&name) {
            (is_continue, ctx) = filter.resp_filter(ctx, matched_match_inst).await?;
            if !is_continue {
                return Ok(ctx);
            }
            executed_filters.push(name);
        }
    }
    Ok(ctx)
}

fn choose_backend(backends: &Vec<SgHttpBackendRef>) -> Option<&SgHttpBackendRef> {
    if backends.is_empty() {
        None
    } else if backends.len() == 1 {
        backends.get(0)
    } else {
        let weights = backends.iter().map(|backend| backend.weight.unwrap_or(0)).collect_vec();
        let dist = WeightedIndex::new(weights).unwrap();
        let mut rng = thread_rng();
        backends.get(dist.sample(&mut rng))
    }
}

pub fn into_http_error(error: TardisError) -> Result<Response<Body>, hyper::Error> {
    let status_code = match error.code.parse::<u16>() {
        Ok(code) => match StatusCode::from_u16(code) {
            Ok(status_code) => status_code,
            Err(_) => {
                if (200..400).contains(&code) {
                    StatusCode::OK
                } else if (400..500).contains(&code) {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }
        },
        Err(_) => {
            if error.code.starts_with('2') || error.code.starts_with('3') {
                StatusCode::OK
            } else if error.code.starts_with('4') {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    };
    let mut response = Response::new(Body::from(
        TardisFuns::json
            .obj_to_string(&SgRespError {
                code: error.code,
                msg: error.message,
            })
            .unwrap(),
    ));
    *response.status_mut() = status_code;
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
    pub client: Client<HttpsConnector<HttpConnector>>,
}

#[derive(Default)]
struct SgHttpRouteInst {
    pub hostnames: Option<Vec<String>>,
    pub filters: Vec<(String, Box<dyn SgPluginFilter>)>,
    pub rules: Option<Vec<SgHttpRouteRuleInst>>,
}

#[derive(Default)]
struct SgHttpRouteRuleInst {
    pub filters: Vec<(String, Box<dyn SgPluginFilter>)>,
    pub matches: Option<Vec<SgHttpRouteMatchInst>>,
    pub backends: Option<Vec<SgHttpBackendRef>>,
    pub timeout: Option<u64>,
}

#[derive(Default)]
pub struct SgHttpRouteMatchInst {
    pub path: Option<SgHttpPathMatchInst>,
    pub header: Option<Vec<SgHttpHeaderMatchInst>>,
    pub query: Option<Vec<SgHttpQueryMatchInst>>,
    pub method: Option<Vec<String>>,
}
#[derive(Default)]

pub struct SgHttpPathMatchInst {
    pub kind: SgHttpPathMatchType,
    pub value: String,
    pub regular: Option<Regex>,
}

#[derive(Default)]
pub struct SgHttpHeaderMatchInst {
    pub kind: SgHttpHeaderMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

#[derive(Default)]
pub struct SgHttpQueryMatchInst {
    pub kind: SgHttpQueryMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use http::Request;
    use hyper::Body;
    use tardis::regex::Regex;

    use crate::{
        config::http_route_dto::{SgHttpBackendRef, SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType},
        functions::http_route::{choose_backend, match_route_insts_with_hostname_priority, SgHttpHeaderMatchInst, SgHttpQueryMatchInst, SgHttpRouteInst},
    };

    use super::{match_rule_inst, SgHttpPathMatchInst, SgHttpRouteMatchInst};

    // #[test]
    // fn test_match_rule_inst() {
    //     assert!(match_rule_inst(&Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(), None));
    //     // -----------------------------------------------------
    //     // Match exact path
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         path: Some(SgHttpPathMatchInst {
    //             kind: SgHttpPathMatchType::Exact,
    //             value: "/iam".to_string(),
    //             regular: None,
    //         }),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     // Match prefix path
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         path: Some(SgHttpPathMatchInst {
    //             kind: SgHttpPathMatchType::Prefix,
    //             value: "/iam".to_string(),
    //             regular: None,
    //         }),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/spi/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     // Match regular path
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         path: Some(SgHttpPathMatchInst {
    //             kind: SgHttpPathMatchType::Regular,
    //             value: "/iam/[0-9]+/hi".to_string(),
    //             regular: Some(Regex::new("/iam/[0-9]+/hi").unwrap()),
    //         }),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct/hi").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/001/hi/hi").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/001/hi/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));

    //     // -----------------------------------------------------
    //     // Match method
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         method: Some(vec!["get".to_string()]),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().method("post").uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));

    //     // -----------------------------------------------------
    //     // Match exact header
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         header: Some(vec![
    //             SgHttpHeaderMatchInst {
    //                 kind: SgHttpHeaderMatchType::Exact,
    //                 name: "X-Auth-User".to_string(),
    //                 value: "gdxr".to_string(),
    //                 regular: None,
    //             },
    //             SgHttpHeaderMatchInst {
    //                 kind: SgHttpHeaderMatchType::Exact,
    //                 name: "App".to_string(),
    //                 value: "a001".to_string(),
    //                 regular: None,
    //             },
    //         ]),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("Tenant", "t001").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "t001").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "a001").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("X-Auth-User", "gdxr").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("app", "a001").header("X-auTh-User", "gdxr").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));

    //     // Match regular header
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         header: Some(vec![SgHttpHeaderMatchInst {
    //             kind: SgHttpHeaderMatchType::Regular,
    //             name: "X-Id".to_string(),
    //             value: "^[0-9]+$".to_string(),
    //             regular: Some(Regex::new("^[0-9]+$").unwrap()),
    //         }]),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("x-iD", "t001").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/iam/ct").header("x-iD", "002").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));

    //     // -----------------------------------------------------
    //     // Match exact query
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         query: Some(vec![
    //             SgHttpQueryMatchInst {
    //                 kind: SgHttpQueryMatchType::Exact,
    //                 name: "id".to_string(),
    //                 value: "gdxr".to_string(),
    //                 regular: None,
    //             },
    //             SgHttpQueryMatchInst {
    //                 kind: SgHttpQueryMatchType::Exact,
    //                 name: "name".to_string(),
    //                 value: "星航".to_string(),
    //                 regular: None,
    //             },
    //         ]),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?id=gdxr").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA%26id%3Dgdxr").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?name%3D%E6%98%9F%E8%88%AA%26id%3Dgdxr%26code%3D1").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));

    //     // Match regular query
    //     let match_conds = vec![SgHttpRouteMatchInst {
    //         query: Some(vec![SgHttpQueryMatchInst {
    //             kind: SgHttpQueryMatchType::Regular,
    //             name: "id".to_string(),
    //             value: "id[a-z]+".to_string(),
    //             regular: Some(Regex::new("id[a-z]+").unwrap()),
    //         }]),
    //         ..Default::default()
    //     }];
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?id=gdxr").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(!match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?id=idAdef").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    //     assert!(match_rule_inst(
    //         &Request::builder().uri("https://sg.idealworld.group/?id=idadef").body(Body::empty()).unwrap(),
    //         Some(&match_conds)
    //     ));
    // }

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
            choose_backend(&vec![SgHttpBackendRef {
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
            SgHttpBackendRef {
                name_or_host: "iam1".to_string(),
                weight: Some(30),
                ..Default::default()
            },
            SgHttpBackendRef {
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
