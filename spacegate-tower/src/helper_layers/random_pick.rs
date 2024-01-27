use std::sync::Arc;

use rand::distributions::Distribution;

#[derive(Clone)]
pub struct RandomPick<I, S>
where I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd
{
    picker: Arc<rand::distributions::WeightedIndex<I>>,
    services: Arc<[S]>,
}

impl<I, S> RandomPick<I, S>
where I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd + Clone + Default + for<'a> std::ops::AddAssign<&'a I>
{
    pub fn new(services: impl IntoIterator<Item = (I, S)>) -> Self {
        let (weights, services): (Vec<_>, Vec<_>) = services.into_iter().unzip();
        assert!(!services.is_empty(), "services must not be empty");
        Self {
            picker: Arc::new(rand::distributions::WeightedIndex::new(weights).expect("invalid weights")),
            services: services.into(),
        }
    }
}

impl<I, R, S> hyper::service::Service<R> for RandomPick<I, S>
where
    S: hyper::service::Service<R>,
    S::Future: std::marker::Send,
    I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd  + Clone
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, req: R) -> Self::Future {
        let index = self.picker.sample(&mut rand::thread_rng());
        self.services[index].call(req)
    }
}
