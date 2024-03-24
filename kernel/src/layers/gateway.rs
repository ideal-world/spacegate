pub mod builder;

use std::{cell::RefCell, collections::HashMap, convert::Infallible, ops::Index, rc::Rc, sync::Arc};

use crate::{
    extension::{GatewayName, MatchedSgRouter},
    helper_layers::{
        map_request::{add_extension::add_extension, MapRequestLayer},
        reload::Reloader,
        route::{Route, Router},
    },
    service::BoxHyperService,
    SgBody, SgBoxLayer,
};

use hyper::{header::HOST, Request, Response};
use tokio_util::sync::CancellationToken;

use tower_layer::Layer;
use tracing::{debug, instrument};

use super::http_route::{match_hostname::HostnameTree, match_request::MatchRequest, SgHttpRoute, SgHttpRouter};

/****************************************************************************************

                                          Gateway

*****************************************************************************************/

pub type SgGatewayRoute = Route<SgGatewayRoutedServices, SgGatewayRouter, BoxHyperService>;

#[derive(Debug, Clone)]
pub struct SgGatewayLayer {
    pub gateway_name: Arc<str>,
    pub http_routes: HashMap<String, SgHttpRoute>,
    pub http_plugins: Vec<SgBoxLayer>,
    pub http_fallback: SgBoxLayer,
    pub http_route_reloader: Reloader<SgGatewayRoute>,
}

impl SgGatewayLayer {
    /// Create a new gateway layer.
    /// # Arguments
    /// * `gateway_name` - The gateway name, this may be used by plugins.
    /// * `cancel_token` - A cancel token hints wether the gateway server is still alive.
    ///
    pub fn builder(gateway_name: impl Into<Arc<str>>, cancel_token: CancellationToken) -> builder::SgGatewayLayerBuilder {
        builder::SgGatewayLayerBuilder::new(gateway_name, cancel_token)
    }
}

#[derive(Debug, Clone)]
pub struct SgGatewayRoutedServices {
    services: Arc<[Vec<BoxHyperService>]>,
}

#[derive(Debug, Clone)]
pub struct SgGatewayRouter {
    pub routers: Arc<[SgHttpRouter]>,
    pub hostname_tree: Arc<HostnameTree<Vec<(usize, i16)>>>,
}

impl Index<(usize, usize)> for SgGatewayRoutedServices {
    type Output = BoxHyperService;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.services.as_ref()[index.0][index.1]
    }
}
impl Router for SgGatewayRouter {
    type Index = (usize, usize);
    #[instrument(skip_all, fields(uri = req.uri().to_string(), method = req.method().as_str(), host = ?req.headers().get(HOST) ))]
    fn route(&self, req: &mut Request<SgBody>) -> Option<Self::Index> {
        let host = req.headers().get(HOST).and_then(|x| x.to_str().ok())?;
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
        None
    }
}

impl<S> Layer<S> for SgGatewayLayer
where
    S: Clone + hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = BoxHyperService;

    fn layer(&self, inner: S) -> Self::Service {
        let gateway_name = GatewayName::new(self.gateway_name.clone());
        let add_gateway_name_layer = MapRequestLayer::new(add_extension(gateway_name, true));
        let gateway_plugins = self.http_plugins.iter().collect::<SgBoxLayer>();
        let http_routes = self.http_routes.values().cloned().collect::<Vec<_>>();
        let route = create_http_router(&http_routes, &self.http_fallback, inner);
        #[cfg(feature = "reload")]
        let service = {
            let reloader = self.http_route_reloader.clone();
            reloader.into_layer().layer(route)
        };
        #[cfg(not(feature = "reload"))]
        let service = route;
        BoxHyperService::new(add_gateway_name_layer.layer(gateway_plugins.layer(service)))
    }
}

pub fn create_http_router<S>(routes: &[SgHttpRoute], fallback: &SgBoxLayer, inner: S) -> Route<SgGatewayRoutedServices, SgGatewayRouter, BoxHyperService>
where
    S: Clone + hyper::service::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as hyper::service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    let mut services = Vec::with_capacity(routes.len());
    let mut routers = Vec::with_capacity(routes.len());
    let mut hostname_tree = HostnameTree::<Vec<_>>::new();
    for (idx, route) in routes.iter().enumerate() {
        let priority = route.priority;
        let idx_with_priority = (idx, priority);
        let route_plugins = route.plugins.iter().collect::<SgBoxLayer>();
        let mut rules_services = Vec::with_capacity(route.rules.len());
        let mut rules_router = Vec::with_capacity(route.rules.len());
        for rule in route.rules.iter() {
            // let rule_service = route_plugins.layer(rule.layer(inner.clone()));
            let rule_service = route_plugins.layer(rule.layer(inner.clone()));
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
        routers.push(SgHttpRouter {
            hostnames: route.hostnames.clone().into(),
            rules: rules_router.into_iter().map(|x| x.map(|v| v.into_iter().map(Arc::new).collect::<Arc<[_]>>())).collect(),
        });
    }

    // sort the indices by priority
    // we put the highest priority at the front of the vector
    hostname_tree.iter_mut().for_each(|indices| indices.sort_unstable_by_key(|(_, p)| i16::MAX - *p));
    debug!("hostname_tree: {hostname_tree:?}");
    Route::new(
        SgGatewayRoutedServices { services: services.into() },
        SgGatewayRouter {
            routers: routers.into(),
            hostname_tree: Arc::new(hostname_tree),
        },
        fallback.layer(inner),
    )
}
