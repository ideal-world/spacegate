use std::ops::{Deref, DerefMut};

use hyper::http::Extensions;

/// Reflect is a wrapper around `hyper::http::Extensions`
///
/// The extensions in reflect will be passed to the corresponded response if request is sent out from backend.
#[derive(Clone, Default, Debug)]
#[repr(transparent)]
pub struct Reflect(Extensions);

impl Deref for Reflect {
    type Target = Extensions;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Reflect {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Reflect {
    pub fn new() -> Self {
        Self(Extensions::new())
    }
    pub fn into_inner(self) -> Extensions {
        self.0
    }
}

impl From<Extensions> for Reflect {
    fn from(ext: Extensions) -> Self {
        Self(ext)
    }
}
