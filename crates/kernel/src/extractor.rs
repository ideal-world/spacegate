use hyper::Request;

use crate::SgBody;
pub mod extension;
pub mod header;

/// a marker is some information that can be attached to a request and can be extracted from a request.
pub trait Extractor: Sized + Send + Sync + 'static {
    fn extract(req: &Request<SgBody>) -> Option<Self>;
}
