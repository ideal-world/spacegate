use rand::{self, distributions::Distribution};
use tower::steer::Picker;

use crate::{Request, SgBody};

use super::{match_request::MatchRequest, SgHttpBackend, SgRouteRule};

#[derive(Debug, Clone, Copy)]
pub struct RouteByWeight;

impl<S, R> Picker<SgHttpBackend<S>, R> for RouteByWeight {
    fn pick(&mut self, _r: &R, services: &[SgHttpBackend<S>]) -> usize {
        let weights = services.iter().map(|x| x.weight);
        let Ok(weighted) = rand::distributions::WeightedIndex::new(weights) else { return 0 };
        weighted.sample(&mut rand::thread_rng())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RouteByMatches {
    pub fallback_index: usize,
}

impl Picker<SgRouteRule, Request<SgBody>> for RouteByMatches {
    fn pick(&mut self, r: &Request<SgBody>, services: &[SgRouteRule]) -> usize {
        for (i, service) in services.iter().enumerate() {
            if self.fallback_index != i && service.r#match.match_request(r) {
                return i;
            }
        }
        self.fallback_index
    }
}
