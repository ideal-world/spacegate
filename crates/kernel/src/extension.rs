mod reflect;
use hyper::http::Extensions;
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
mod is_east_west_traffic;
pub mod user_group;
/// Just extract and attach the extension to the request
#[derive(Debug, Clone)]
pub struct Extension<E>(E);

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

pub trait ExtensionPack: Sized {
    fn insert(self, ext: &mut Extensions) -> Option<Self>
    where
        Self: Clone + Send + Sync + 'static,
    {
        ext.insert::<Self>(self)
    }

    fn get(ext: &Extensions) -> Option<&Self>
    where
        Self: Send + Sync + 'static,
    {
        ext.get::<Self>()
    }

    fn get_mut(ext: &mut Extensions) -> Option<&mut Self>
    where
        Self: Send + Sync + 'static,
    {
        ext.get_mut::<Self>()
    }

    fn remove(ext: &mut Extensions) -> Option<Self>
    where
        Self: Send + Sync + 'static,
    {
        ext.remove::<Self>()
    }
}
