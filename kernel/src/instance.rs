use crate::config::{
    gateway_dto::{SgListener, SgProtocol},
    http_route_dto::{SgHttpHeaderMatchType, SgHttpPathMatchType, SgHttpQueryMatchType},
};

use http::Method;
use hyper_rustls::HttpsConnector;

use std::{fmt, vec::Vec};
use tardis::regex::Regex;

pub(crate) struct SgGatewayInst {
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub routes: Vec<SgHttpRouteInst>,
    pub client: Client<HttpsConnector<HttpConnector>>,
    pub listeners: Vec<SgListener>,
}

#[derive(Default)]
pub struct SgHttpRouteInst {
    pub hostnames: Option<Vec<String>>,
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub rules: Option<Vec<SgHttpRouteRuleInst>>,
}

impl fmt::Display for SgHttpRouteInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rules = if let Some(rules) = &self.rules {
            format!("rules:[{}]", rules.iter().map(|r| format!("{}", r)).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        match &self.hostnames {
            Some(hostnames) => {
                if hostnames.is_empty() {
                    write!(f, "{}", rules)
                } else {
                    write!(f, "{} hostnames:[{}]", rules, hostnames.join(", "))
                }
            }
            None => write!(f, "{}", rules),
        }
    }
}

#[derive(Default)]
pub struct SgHttpRouteRuleInst {
    pub filters: Vec<(String, BoxSgPluginFilter)>,
    pub matches: Option<Vec<SgHttpRouteMatchInst>>,
    pub backends: Option<Vec<SgBackendInst>>,
    pub timeout_ms: Option<u64>,
}

impl fmt::Display for SgHttpRouteRuleInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let matches = if let Some(matches) = &self.matches {
            format!("matches:[{}]", matches.iter().map(|m| format!("{}", m)).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        let backend = if let Some(backend) = &self.backends {
            format!("backend:[{}]", backend.iter().map(|b| format!("({})", b)).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        match self.timeout_ms {
            Some(t) => write!(f, "{} timeout:{}", format!("{}=>{}", matches, backend).trim(), t),
            None => write!(f, "{}=>{}", matches, backend),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpRouteMatchInst {
    pub path: Option<SgHttpPathMatchInst>,
    pub header: Option<Vec<SgHttpHeaderMatchInst>>,
    pub query: Option<Vec<SgHttpQueryMatchInst>>,
    // here method should be Method
    pub method: Option<Vec<Method>>,
}

impl fmt::Display for SgHttpRouteMatchInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if let Some(path) = &self.path { format!("path:{}", path) } else { "".to_string() };
        let header = if let Some(header) = &self.header {
            format!("header:[{}]", header.iter().map(|h| format!("{}", h)).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        let query = if let Some(query) = &self.query {
            format!("query:[{}]", query.iter().map(|q| format!("{}", q)).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        let method = if let Some(method) = &self.method {
            format!("method:[{}]", method.iter().map(|m| m.to_string()).collect::<Vec<String>>().join(", "))
        } else {
            "".to_string()
        };
        write!(f, "{}", format!("{} {} {} {}", path, header, query, method).trim())
    }
}
#[derive(Default, Debug, Clone)]

pub struct SgHttpPathMatchInst {
    pub kind: SgHttpPathMatchType,
    pub value: String,
    pub regular: Option<Regex>,
}

impl fmt::Display for SgHttpPathMatchInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.regular {
            Some(reg) => write!(f, "{} {} {}", self.kind, reg, self.value),
            None => write!(f, "{} {}", self.kind, self.value),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpHeaderMatchInst {
    pub kind: SgHttpHeaderMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

impl fmt::Display for SgHttpHeaderMatchInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.regular {
            Some(reg) => write!(f, "{}: {} {} {}", self.name, self.kind, reg, self.value),
            None => write!(f, "{}: {} {} ", self.name, self.kind, self.value),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct SgHttpQueryMatchInst {
    pub kind: SgHttpQueryMatchType,
    pub name: String,
    pub value: String,
    pub regular: Option<Regex>,
}

impl fmt::Display for SgHttpQueryMatchInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.regular {
            Some(reg) => write!(f, "{}= {} {} {}", self.name, self.kind, reg, self.value),
            None => write!(f, "{}= {} {} ", self.name, self.kind, self.value),
        }
    }
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

impl fmt::Display for SgBackendInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = format!(
            "{}://{}{}:{}",
            self.protocol.as_ref().unwrap_or(&SgProtocol::Http),
            self.name_or_host,
            self.namespace.as_ref().map(|n| format!(".{n}")).unwrap_or("".to_string()),
            self.port
        );
        let timeout_ms = if let Some(t) = self.timeout_ms { format!(",timeout({})", t) } else { "".to_string() };
        write!(f, "weight({}){timeout_ms}->{url}", self.weight.as_ref().unwrap_or(&0),)
    }
}
