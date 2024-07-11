pub mod ip_hash;
pub use ip_hash::IpHash;
pub mod random;
pub use random::Random;
#[derive(Debug, Clone, Default)]
pub struct Balancer<P, S> {
    pub policy: P,
    pub instances: Vec<S>,
    pub fallback: S,
}

impl<P, S> Balancer<P, S> {
    pub fn new(policy: P, instances: Vec<S>, fallback: S) -> Self {
        Self { policy, instances, fallback }
    }
}

pub trait BalancePolicy<S, R> {
    fn pick<'s>(&self, instances: &'s [S], req: &R) -> Option<&'s S>;
}

impl<P, R, S> hyper::service::Service<R> for Balancer<P, S>
where
    P: BalancePolicy<S, R>,
    S: hyper::service::Service<R>,
    S::Future: std::marker::Send,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = S::Future;

    fn call(&self, req: R) -> Self::Future {
        self.policy.pick(&self.instances, &req).unwrap_or(&self.fallback).call(req)
    }
}
