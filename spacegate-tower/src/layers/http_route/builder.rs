use std::{num::NonZeroU16, sync::Arc, time::Duration};

use tower::BoxError;

use crate::SgBoxLayer;

use super::{match_request::SgHttpRouteMatch, SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer};

#[derive(Debug)]
pub struct SgHttpRouteLayerBuilder {
    pub hostnames: Vec<String>,
    pub rules: Vec<SgHttpRouteRuleLayer>,
    pub plugins: Vec<SgBoxLayer>,
}

impl Default for SgHttpRouteLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgHttpRouteLayerBuilder {
    pub fn new() -> Self {
        Self {
            hostnames: Vec::new(),
            rules: Vec::new(),
            plugins: Vec::new(),
        }
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
    pub fn build(mut self) -> Result<SgHttpRoute, BoxError> {
        if self.hostnames.iter().any(|host| host == "*") {
            self.hostnames = vec!["*".to_string()]
        }
        Ok(SgHttpRoute {
            plugins: Arc::from(self.plugins),
            hostnames: self.hostnames.into(),
            rules: self.rules.into(),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpRouteRuleLayerBuilder {
    r#match: Option<Vec<SgHttpRouteMatch>>,
    plugins: Vec<SgBoxLayer>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayer>,
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
        Ok(SgHttpRouteRuleLayer {
            r#match: self.r#match.into(),
            plugins: Arc::from(self.plugins),
            timeouts: self.timeouts,
            backends: Arc::from_iter(self.backends),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpBackendLayerBuilder {
    host: Option<String>,
    port: Option<NonZeroU16>,
    protocol: Option<String>,
    plugins: Vec<SgBoxLayer>,
    timeout: Option<Duration>,
    weight: u16,
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
        self.port = NonZeroU16::new(port);
        self
    }
    pub fn protocol(mut self, protocol: String) -> Self {
        self.protocol = Some(protocol);
        self
    }
    pub fn build(self) -> Result<SgHttpBackendLayer, BoxError> {
        Ok(SgHttpBackendLayer {
            host: self.host.map(Into::into),
            port: self.port,
            scheme: self.protocol.map(Into::into),
            filters: Arc::from(self.plugins),
            timeout: self.timeout,
            weight: self.weight,
        })
    }
}
