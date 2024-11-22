use hyper::Request;

use crate::{BoxResult, SgBody};

pub trait Inject {
    /// Inject the request with some data.
    /// # Errors
    /// If the injection fails.
    fn inject(&self, req: &mut Request<SgBody>) -> BoxResult<()>;
}
