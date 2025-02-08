use std::sync::Arc;

use rand::distr::Distribution;

#[derive(Clone)]
pub struct RandomPick<I, S>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd,
{
    picker: Arc<rand::distr::weighted::WeightedIndex<I>>,
    services: Arc<[S]>,
}

impl<I, S> std::fmt::Debug for RandomPick<I, S>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RandomPick").finish()
    }
}

impl<I, S> RandomPick<I, S>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd + Clone + Default + for<'a> std::ops::AddAssign<&'a I> + rand::distr::weighted::Weight,
{
    pub fn new(services: impl IntoIterator<Item = (I, S)>) -> Self {
        let (weights, services): (Vec<_>, Vec<_>) = services.into_iter().unzip();
        assert!(!services.is_empty(), "services must not be empty");
        Self {
            picker: Arc::new(rand::distr::weighted::WeightedIndex::new(weights).expect("invalid weights")),
            services: services.into(),
        }
    }
}

impl<I, R, S> hyper::service::Service<R> for RandomPick<I, S>
where
    S: hyper::service::Service<R>,
    S::Future: std::marker::Send,
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[allow(clippy::indexing_slicing)]
    fn call(&self, req: R) -> Self::Future {
        if self.services.len() == 1 {
            self.services[0].call(req)
        } else {
            let index = self.picker.sample(&mut rand::thread_rng());
            self.services[index].call(req)
        }
    }
}
