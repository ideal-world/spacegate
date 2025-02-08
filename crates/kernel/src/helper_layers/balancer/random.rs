use rand::distr::Distribution;

use super::BalancePolicy;
/// A policy that selects an instance randomly.
pub struct Random<I>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd,
{
    picker: rand::distr::weighted::WeightedIndex<I>,
}

impl<I> std::fmt::Debug for Random<I>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Random").finish()
    }
}

impl<I> Random<I>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd + Clone + Default + for<'a> std::ops::AddAssign<&'a I> + rand::distr::weighted::Weight,
{
    pub fn new(weights: impl IntoIterator<Item = I>) -> Self {
        Self {
            picker: rand::distr::weighted::WeightedIndex::new(weights).expect("invalid weights"),
        }
    }
}

impl<I, S, R> BalancePolicy<S, R> for Random<I>
where
    I: rand::distr::uniform::SampleUniform + std::cmp::PartialOrd,
{
    fn pick<'s>(&self, instances: &'s [S], _req: &R) -> Option<&'s S> {
        if instances.is_empty() {
            None
        } else if instances.len() == 1 {
            instances.first()
        } else {
            let index = self.picker.sample(&mut rand::thread_rng());
            instances.get(index)
        }
    }
}
