use std::time::Duration;

use crate::BoxError;

use crate::SgBoxLayer;

use super::{match_request::SgHttpRouteMatch, SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer};

#[derive(Debug)]
pub struct SgHttpRouteLayerBuilder {
    pub name: String,
    pub hostnames: Vec<String>,
    pub rules: Vec<SgHttpRouteRuleLayer>,
    pub plugins: Vec<SgBoxLayer>,
    pub priority: Option<i16>,
    pub extensions: hyper::http::Extensions,
}

impl Default for SgHttpRouteLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgHttpRouteLayerBuilder {
    pub fn new() -> Self {
        Self {
            name: Default::default(),
            hostnames: Vec::new(),
            rules: Vec::new(),
            plugins: Vec::new(),
            priority: None,
            extensions: Default::default(),
        }
    }
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
    pub fn hostnames(mut self, hostnames: impl IntoIterator<Item = String>) -> Self {
        self.hostnames = hostnames.into_iter().collect();
        self
    }
    pub fn rule(mut self, rule: SgHttpRouteRuleLayer) -> Self {
        self.rules.push(rule);
        self
    }
    pub fn rules(mut self, rules: impl IntoIterator<Item = SgHttpRouteRuleLayer>) -> Self {
        self.rules.extend(rules);
        self
    }
    pub fn plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = SgBoxLayer>) -> Self {
        self.plugins.extend(plugins);
        self
    }
    pub fn priority(mut self, priority: i16) -> Self {
        self.priority = Some(priority);
        self
    }
    pub fn ext(mut self, extensions: hyper::http::Extensions) -> Self {
        self.extensions = extensions;
        self
    }
    pub fn build(mut self) -> Result<SgHttpRoute, BoxError> {
        if self.hostnames.iter().any(|host| host == "*") {
            self.hostnames = vec!["*".to_string()]
        }
        Ok(SgHttpRoute {
            plugins: self.plugins,
            hostnames: self.hostnames,
            rules: self.rules,
            priority: self.priority.unwrap_or(1),
            name: self.name,
            ext: self.extensions,
        })
    }
}

#[derive(Debug)]
pub struct SgHttpRouteRuleLayerBuilder {
    r#match: Option<Vec<SgHttpRouteMatch>>,
    pub plugins: Vec<SgBoxLayer>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayer>,
    pub extensions: hyper::http::Extensions,
}
impl Default for SgHttpRouteRuleLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgHttpRouteRuleLayerBuilder {
    pub fn new() -> Self {
        Self {
            r#match: None,
            plugins: Vec::new(),
            timeouts: None,
            backends: Vec::new(),
            extensions: Default::default(),
        }
    }
    pub fn matches(mut self, matches: impl IntoIterator<Item = SgHttpRouteMatch>) -> Self {
        self.r#match = Some(matches.into_iter().collect());
        self
    }
    pub fn match_all(mut self) -> Self {
        self.r#match = None;
        self
    }
    pub fn plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = SgBoxLayer>) -> Self {
        self.plugins.extend(plugins);
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeouts = Some(timeout);
        self
    }
    pub fn backend(mut self, backend: SgHttpBackendLayer) -> Self {
        self.backends.push(backend);
        self
    }
    pub fn backends(mut self, backend: impl IntoIterator<Item = SgHttpBackendLayer>) -> Self {
        self.backends.extend(backend);
        self
    }
    pub fn build(self) -> Result<SgHttpRouteRuleLayer, BoxError> {
        // let r#match = self.r#match.map(|ms| ms.into_iter().map(Arc::new).collect::<Vec>());
        Ok(SgHttpRouteRuleLayer {
            r#match: self.r#match,
            plugins: self.plugins,
            timeouts: self.timeouts,
            backends: self.backends,
            ext: self.extensions,
        })
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
}

#[derive(Debug)]
pub struct SgHttpBackendLayerBuilder {
    host: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    pub plugins: Vec<SgBoxLayer>,
    timeout: Option<Duration>,
    weight: u16,
    pub extensions: hyper::http::Extensions,
}

impl Default for SgHttpBackendLayerBuilder {
    fn default() -> Self {
        Self {
            host: None,
            port: None,
            protocol: None,
            plugins: Vec::new(),
            timeout: None,
            weight: 1,
            extensions: Default::default(),
        }
    }
}

impl SgHttpBackendLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = SgBoxLayer>) -> Self {
        self.plugins.extend(plugins);
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }
    pub fn protocol(mut self, protocol: String) -> Self {
        self.protocol = Some(protocol);
        self
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
    pub fn build(self) -> Result<SgHttpBackendLayer, BoxError> {
        Ok(SgHttpBackendLayer {
            host: self.host.map(Into::into),
            port: self.port,
            scheme: self.protocol.map(Into::into),
            plugins: self.plugins,
            timeout: self.timeout,
            weight: self.weight,
            ext: self.extensions,
        })
    }
}
