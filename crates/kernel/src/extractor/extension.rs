use hyper::Request;

use crate::{extension::Extension, Extract, SgBody};

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
            Self::new(Some(ext.inner().clone()))
        } else {
            Self::new(None)
        }
    }
}
