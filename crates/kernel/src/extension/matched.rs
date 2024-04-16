use crate::{helper_layers::route::Router, layers::http_route::match_request::HttpRouteMatch};
use std::{ops::Deref, sync::Arc};

#[derive(Debug, Clone)]
pub struct Matched<R: Router> {
    pub router: R,
    pub index: R::Index,
}

#[derive(Debug, Clone)]
pub struct MatchedSgRouter(pub Arc<HttpRouteMatch>);

impl Deref for MatchedSgRouter {
    type Target = HttpRouteMatch;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}
