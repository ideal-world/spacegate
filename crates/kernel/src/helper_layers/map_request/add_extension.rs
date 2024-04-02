use std::any::Any;

use crate::{extension::Reflect, SgBody};
use hyper::Request;

pub fn add_extension<E: Any + Clone + Send + Sync + 'static>(extension: E, reflect: bool) -> (impl (Fn(Request<SgBody>) -> Request<SgBody>) + Clone) {
    move |mut req: Request<SgBody>| {
        req.extensions_mut().insert(extension.clone());
        if reflect {
            req.extensions_mut().get_mut::<Reflect>().map(|r| r.insert(extension.clone()));
        }
        req
    }
}
