use hyper::Request;

use crate::SgBody;
pub mod extension;
pub mod header;

/// a marker is some information that can be attached to a request and can be extracted from a request.
pub trait Marker: Sized + Send + Sync + 'static {
    fn extract(req: &Request<SgBody>) -> Option<Self>;
    fn attach(self, req: &mut Request<SgBody>);
    fn detach(req: &mut Request<SgBody>) -> Option<Self>;
}
