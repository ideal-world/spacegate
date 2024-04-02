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
