pub mod builder;
use std::{collections::HashMap, ops::Index, sync::Arc};

use crate::{
    backend_service::ArcHyperService,
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::{
        map_request::{add_extension::add_extension, MapRequestLayer},
        reload::Reloader,
        route::{Router, RouterService},
    },
    utils::fold_box_layers::fold_layers,
    BoxLayer, SgBody,
};

use hyper::{header::HOST, Request};

use tower_layer::Layer;
use tracing::{debug, instrument};

use super::http_route::{match_hostname::HostnameTree, match_request::MatchRequest, HttpRoute, HttpRouter};

/****************************************************************************************

                                          Gateway

*****************************************************************************************/

pub type HttpRouterService = RouterService<HttpRoutedService, GatewayRouter, ArcHyperService>;

#[derive(Debug)]
pub struct Gateway {
    pub gateway_name: Arc<str>,
    pub http_routes: HashMap<String, HttpRoute>,
    pub http_plugins: Vec<BoxLayer>,
    pub http_fallback: ArcHyperService,
    pub http_route_reloader: Reloader<HttpRouterService>,
    pub ext: hyper::http::Extensions,
}

impl Gateway {
    /// Create a new gateway layer.
    /// # Arguments
    /// * `gateway_name` - The gateway name, this may be used by plugins.
    pub fn builder(gateway_name: impl Into<Arc<str>>) -> builder::GatewayBuilder {
        builder::GatewayBuilder::new(gateway_name)
    }
    pub fn as_service(&self) -> ArcHyperService {
        let gateway_name = GatewayName::new(self.gateway_name.clone());
        let add_gateway_name_layer = MapRequestLayer::new(add_extension(gateway_name, true));
        let gateway_plugins = self.http_plugins.iter();
        let http_routes = self.http_routes.values();
        let route = create_http_router(http_routes, self.http_fallback.clone());
        #[cfg(feature = "reload")]
        let service = {
            let reloader = self.http_route_reloader.clone();
            reloader.into_layer().layer(route)
        };
        #[cfg(not(feature = "reload"))]
        let service = route;
        ArcHyperService::new(add_gateway_name_layer.layer(fold_layers(gateway_plugins, ArcHyperService::new(service))))
    }
}

#[derive(Debug, Clone)]
pub struct HttpRoutedService {
    services: Arc<[Vec<ArcHyperService>]>,
}

#[derive(Debug, Clone)]
pub struct GatewayRouter {
    pub routers: Arc<[HttpRouter]>,
    pub hostname_tree: Arc<HostnameTree<Vec<(usize, i16)>>>,
}

impl Index<(usize, usize)> for HttpRoutedService {
    type Output = ArcHyperService;
    fn index(&self, index: (usize, usize)) -> &Self::Output {
        #[allow(clippy::indexing_slicing)]
        &self.services.as_ref()[index.0][index.1]
    }
}
impl Router for GatewayRouter {
    type Index = (usize, usize);
    #[instrument(skip_all, fields(http.host =? req.headers().get(HOST) ))]
    /// Route the request to the corresponding service.
    /// 
    /// (Maybe it will be radix tree in the future.)
    fn route(&self, req: &mut Request<SgBody>) -> Option<Self::Index> {
        let host = req.uri().host().or(req.headers().get(HOST).and_then(|x| x.to_str().ok()))?;
        let indices = self.hostname_tree.get(host)?;
        for (route_index, _p) in indices {
            for (idx1, matches) in self.routers.as_ref().index(*route_index).rules.iter().enumerate() {
                // tracing::trace!("try match {match:?} [{route_index},{idx1}:{_p}]");
                let index = (*route_index, idx1);
                if let Some(ref matches) = matches {
                    for m in matches.as_ref() {
                        if m.match_request(req) {
                            req.extensions_mut().insert(MatchedSgRouter(m.clone()));
                            tracing::trace!("matches {m:?} [{route_index},{idx1}:{_p}]");
                            if let Err(e) = m.rewrite(req) {
                                tracing::warn!("rewrite failed: {e:?}");
                                return None;
                            }
                            return Some(index);
                        }
                    }
                    continue;
                } else {
                    tracing::trace!("matches wildcard [{route_index},{idx1}:{_p}]");
                    return Some(index);
                }
            }
        }
        tracing::trace!("no rule matched");

        None
    }
}

pub fn create_http_router<'a>(routes: impl Iterator<Item = &'a HttpRoute>, fallback: ArcHyperService) -> RouterService<HttpRoutedService, GatewayRouter, ArcHyperService> {
    let mut services = Vec::new();
    let mut routers = Vec::new();
    let mut hostname_tree = HostnameTree::<Vec<_>>::new();
    for (idx, route) in routes.enumerate() {
        let priority = route.priority;
        let idx_with_priority = (idx, priority);
        // let route_plugins = route.plugins.iter().map(SgRefLayer::new).collect::<SgRefLayer>();
        let mut rules_services = Vec::with_capacity(route.rules.len());
        let mut rules_router = Vec::with_capacity(route.rules.len());
        for rule in route.rules.iter() {
            let rule_service = fold_layers(route.plugins.iter(), ArcHyperService::new(rule.as_service()));
            rules_services.push(rule_service);
            rules_router.push(rule.r#match.clone());
        }
        if route.hostnames.is_empty() {
            if let Some(indices) = hostname_tree.get_mut("*") {
                indices.push(idx_with_priority)
            } else {
                hostname_tree.set("*", vec![idx_with_priority]);
            }
        } else {
            for hostname in route.hostnames.iter() {
                if let Some(indices) = hostname_tree.get_mut(hostname) {
                    indices.push(idx_with_priority)
                } else {
                    hostname_tree.set("*", vec![idx_with_priority]);
                }
            }
        }
        services.push(rules_services);
        routers.push(HttpRouter {
            hostnames: route.hostnames.clone().into(),
            rules: rules_router.into_iter().map(|x| x.map(|v| v.into_iter().map(Arc::new).collect::<Arc<[_]>>())).collect(),
            ext: route.ext.clone(),
        });
    }

    // sort the indices by priority
    // we put the highest priority at the front of the vector
    hostname_tree.iter_mut().for_each(|indices| indices.sort_unstable_by_key(|(_, p)| i16::MAX - *p));
    debug!("hostname_tree: {hostname_tree:?}");
    RouterService::new(
        HttpRoutedService { services: services.into() },
        GatewayRouter {
            routers: routers.into(),
            hostname_tree: Arc::new(hostname_tree),
        },
        fallback,
    )
}
