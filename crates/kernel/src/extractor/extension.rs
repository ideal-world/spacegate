use hyper::Request;

use crate::{Extract, SgBody};

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

impl<E> Extract for Option<Extension<E>>
where
    E: Send + Sync + 'static + Clone,
{
    fn extract(req: &Request<SgBody>) -> Self {
        req.extensions().get::<Extension<E>>().cloned()
    }
}

impl<E> Extract for Extension<Option<E>>
where
    E: Send + Sync + 'static + Clone,
{
    fn extract(req: &Request<SgBody>) -> Self {
        if let Some(ext) = req.extensions().get::<Extension<E>>() {
            Self(Some(ext.0.clone()))
        } else {
            Self(None)
        }
    }
}
