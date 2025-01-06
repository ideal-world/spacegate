mod reflect;
pub use reflect::*;
mod gateway_name;
pub use gateway_name::*;
mod matched;
pub use matched::*;
mod peer_addr;
pub use peer_addr::*;
mod backend_host;
pub use backend_host::*;
mod enter_time;
pub use enter_time::*;
mod request_id;
pub use defer::*;
mod defer;
pub use original_ip_addr::*;
mod original_ip_addr;
pub use is_east_west_traffic::*;

use crate::{extractor::OptionalExtract, injector::Inject};
mod is_east_west_traffic;
pub mod user_group;
/// Just extract and attach the extension to the request
#[derive(Debug, Clone)]
pub struct Extension<E>(pub E);

impl<E> Extension<E> {
    pub const fn new(e: E) -> Self {
        Self(e)
    }

    pub fn into_inner(self) -> E {
        self.0
    }
    pub const fn inner(&self) -> &E {
        &self.0
    }
}

impl<E: Clone + Send + Sync + 'static> Inject for Extension<E> {
    fn inject(&self, req: &mut hyper::Request<crate::SgBody>) -> crate::BoxResult<()> {
        req.extensions_mut().insert(self.0.clone());
        Ok(())
    }
}

impl<E: Clone + Send + Sync + 'static> OptionalExtract for Extension<E> {
    fn extract(req: &hyper::Request<crate::SgBody>) -> Option<Self> {
        req.extensions().get::<E>().map(|e| Self(e.clone()))
    }
}

/// FromBackend is a marker type to indicate that the response is from backend.
#[derive(Debug, Clone, Copy)]
pub struct FromBackend {
    _priv: (),
}

impl FromBackend {
    /// # Safety
    ///
    /// **Ensure** the response is from the **real backend**, do not cheat on users of this type.
    pub const unsafe fn new() -> Self {
        Self { _priv: () }
    }
}
