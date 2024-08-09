use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use crate::SgRequest;

/// It's a hole, don't abuse it pls.
#[derive(Clone, Default)]
pub struct Defer {
    mappers: Arc<Mutex<Vec<Box<dyn FnOnce(SgRequest) -> SgRequest + Send>>>>,
}

impl Debug for Defer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Defer").finish()
    }
}

impl Defer {
    pub fn push_back(&self, f: impl FnOnce(SgRequest) -> SgRequest + Send + 'static) {
        let mut g = self.mappers.lock().expect("never poisoned");
        g.push(Box::new(f))
    }
    pub fn apply(&self, req: SgRequest) -> SgRequest {
        let mut g = self.mappers.lock().expect("never poisoned");
        let mut req = req;
        for f in g.drain(..) {
            req = (f)(req)
        }
        req
    }
}
