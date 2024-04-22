use std::{fmt::Debug, path::PathBuf, time::Duration};

use crate::BoxLayer;

use super::{match_request::HttpRouteMatch, Backend, HttpBackend, HttpRoute, HttpRouteRule};

#[derive(Debug)]
pub struct HttpRouteBuilder {
    pub name: String,
    pub hostnames: Vec<String>,
    pub rules: Vec<HttpRouteRule>,
    pub plugins: Vec<BoxLayer>,
    pub priority: Option<i16>,
    pub extensions: hyper::http::Extensions,
}

impl Default for HttpRouteBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpRouteBuilder {
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
    pub fn rule(mut self, rule: HttpRouteRule) -> Self {
        self.rules.push(rule);
        self
    }
    pub fn rules(mut self, rules: impl IntoIterator<Item = HttpRouteRule>) -> Self {
        self.rules.extend(rules);
        self
    }
    pub fn plugin(mut self, plugin: BoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = BoxLayer>) -> Self {
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
    pub fn build(mut self) -> HttpRoute {
        if self.hostnames.iter().any(|host| host == "*") {
            self.hostnames = vec!["*".to_string()]
        }
        HttpRoute {
            plugins: self.plugins,
            hostnames: self.hostnames,
            rules: self.rules,
            priority: self.priority.unwrap_or(1),
            name: self.name,
            ext: self.extensions,
        }
    }
}

#[derive(Debug)]
pub struct HttpRouteRuleBuilder {
    r#match: Option<Vec<HttpRouteMatch>>,
    pub plugins: Vec<BoxLayer>,
    timeouts: Option<Duration>,
    backends: Vec<HttpBackend>,
    pub extensions: hyper::http::Extensions,
}
impl Default for HttpRouteRuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpRouteRuleBuilder {
    pub fn new() -> Self {
        Self {
            r#match: None,
            plugins: Vec::new(),
            timeouts: None,
            backends: Vec::new(),
            extensions: Default::default(),
        }
    }
    pub fn match_item(mut self, item: impl Into<HttpRouteMatch>) -> Self {
        match self.r#match {
            Some(ref mut matches) => matches.push(item.into()),
            None => self.r#match = Some(vec![item.into()]),
        }
        self
    }
    pub fn matches(mut self, matches: impl IntoIterator<Item = HttpRouteMatch>) -> Self {
        self.r#match = Some(matches.into_iter().collect());
        self
    }
    pub fn match_all(mut self) -> Self {
        self.r#match = None;
        self
    }
    pub fn plugin(mut self, plugin: BoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = BoxLayer>) -> Self {
        self.plugins.extend(plugins);
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeouts = Some(timeout);
        self
    }
    pub fn backend(mut self, backend: HttpBackend) -> Self {
        self.backends.push(backend);
        self
    }
    pub fn backends(mut self, backend: impl IntoIterator<Item = HttpBackend>) -> Self {
        self.backends.extend(backend);
        self
    }
    pub fn build(self) -> HttpRouteRule {
        HttpRouteRule {
            r#match: self.r#match,
            plugins: self.plugins,
            timeouts: self.timeouts,
            backends: self.backends,
            ext: self.extensions,
        }
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
}
pub trait BackendKindBuilder: Default + Debug {
    fn build(self) -> Backend;
}
#[derive(Debug)]
pub struct HttpBackendBuilder<B: BackendKindBuilder = HttpBackendKindBuilder> {
    backend: B,
    pub plugins: Vec<BoxLayer>,
    timeout: Option<Duration>,
    weight: u16,
    pub extensions: hyper::http::Extensions,
}

#[derive(Debug, Default, Clone)]
pub struct HttpBackendKindBuilder {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub schema: Option<String>,
}

impl BackendKindBuilder for HttpBackendKindBuilder {
    fn build(self) -> Backend {
        Backend::Http {
            host: self.host,
            port: self.port,
            schema: self.schema,
        }
    }
}
#[derive(Debug, Default, Clone)]

pub struct FileBackendKindBuilder {
    path: PathBuf,
}

impl BackendKindBuilder for FileBackendKindBuilder {
    fn build(self) -> Backend {
        Backend::File { path: self.path }
    }
}

impl<B: BackendKindBuilder> Default for HttpBackendBuilder<B> {
    fn default() -> Self {
        Self {
            backend: B::default(),
            plugins: Vec::new(),
            timeout: None,
            weight: 1,
            extensions: Default::default(),
        }
    }
}

impl HttpBackendBuilder<FileBackendKindBuilder> {
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.backend = FileBackendKindBuilder { path: path.into() };
        self
    }
}

impl HttpBackendBuilder<HttpBackendKindBuilder> {
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.backend = HttpBackendKindBuilder {
            host: Some(host.into()),
            ..Default::default()
        };
        self
    }
    pub fn port(mut self, port: u16) -> Self {
        self.backend = HttpBackendKindBuilder {
            port: Some(port),
            ..Default::default()
        };
        self
    }
    pub fn schema(mut self, schema: impl Into<String>) -> Self {
        self.backend = HttpBackendKindBuilder {
            schema: Some(schema.into()),
            ..Default::default()
        };
        self
    }
}

impl<B: BackendKindBuilder> HttpBackendBuilder<B> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn plugin(mut self, plugin: BoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = BoxLayer>) -> Self {
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
    pub fn http(self) -> HttpBackendBuilder<HttpBackendKindBuilder> {
        HttpBackendBuilder {
            backend: HttpBackendKindBuilder::default(),
            plugins: self.plugins,
            timeout: self.timeout,
            weight: self.weight,
            extensions: self.extensions,
        }
    }
    pub fn file(self) -> HttpBackendBuilder<FileBackendKindBuilder> {
        HttpBackendBuilder {
            backend: FileBackendKindBuilder::default(),
            plugins: self.plugins,
            timeout: self.timeout,
            weight: self.weight,
            extensions: self.extensions,
        }
    }
    pub fn ext(mut self, extension: hyper::http::Extensions) -> Self {
        self.extensions = extension;
        self
    }
    pub fn build(self) -> HttpBackend {
        HttpBackend {
            backend: self.backend.build(),
            plugins: self.plugins,
            timeout: self.timeout,
            weight: self.weight,
            ext: self.extensions,
        }
    }
}
