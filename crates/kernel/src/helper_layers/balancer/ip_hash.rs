use crate::{extension::PeerAddr, SgRequest};

use super::BalancePolicy;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
};

/// A policy that selects an instance based on the hash of the IP address.
#[derive(Debug, Clone)]
pub struct IpHash<H = DefaultHasher> {
    hasher: PhantomData<fn() -> H>,
}

impl Default for IpHash {
    fn default() -> Self {
        Self { hasher: PhantomData }
    }
}

impl<S, H> BalancePolicy<S, SgRequest> for IpHash<H>
where
    H: Hasher + Default,
{
    fn pick<'s>(&self, instances: &'s [S], req: &SgRequest) -> Option<&'s S> {
        if instances.is_empty() {
            None
        } else if instances.len() == 1 {
            instances.first()
        } else {
            let mut hasher = H::default();
            let ip = req.extensions().get::<PeerAddr>()?.0.ip();
            ip.to_canonical().hash(&mut hasher);
            let hash = hasher.finish();
            let index = (hash % instances.len() as u64) as usize;
            instances.get(index)
        }
    }
}
