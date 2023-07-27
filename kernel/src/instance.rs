use std::{collections::HashMap, fmt, net::SocketAddr, path::Display};

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

// impl fmt::Display for SgHttpHeaderMatchInst {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}, {}", self.kind, self.name)
//     }
// }

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
