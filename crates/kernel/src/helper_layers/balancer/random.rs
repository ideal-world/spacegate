use rand::distributions::Distribution;

use super::BalancePolicy;

/// A policy that selects an instance randomly.
pub struct Random<I>
where
    I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd,
{
    picker: rand::distributions::WeightedIndex<I>,
}

impl<I> Random<I>
where
    I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd + Clone + Default + for<'a> std::ops::AddAssign<&'a I>,
{
    pub fn new(weights: impl IntoIterator<Item = I>) -> Self {
        Self {
            picker: rand::distributions::WeightedIndex::new(weights).expect("invalid weights"),
        }
    }
}

impl<I, S, R> BalancePolicy<S, R> for Random<I>
where
    I: rand::distributions::uniform::SampleUniform + std::cmp::PartialOrd,
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
