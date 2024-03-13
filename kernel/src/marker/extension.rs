use hyper::Request;

use crate::{Marker, SgBody};

/// Just extract and attach the extension to the request
#[derive(Debug, Clone)]
pub struct Extension<E>(E);

impl<E> Extension<E> {
    pub fn new(e: E) -> Self {
        Self(e)
    }

    pub fn into_inner(self) -> E {
        self.0
    }
}

impl<E> Marker for Extension<E>
where
    E: Send + Sync + 'static + Clone,
{
    fn extract(req: &Request<SgBody>) -> Option<Self> {
        req.extensions().get::<E>().cloned().map(Extension)
    }

    fn attach(self, req: &mut Request<SgBody>) {
        req.extensions_mut().insert(Extension(self));
    }

    fn detach(req: &mut Request<SgBody>) -> Option<Self> {
        req.extensions_mut().remove::<E>().map(Extension)
    }
}
