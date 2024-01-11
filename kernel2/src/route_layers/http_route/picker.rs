use tardis::rand::{self, distributions::Distribution};
use tower::steer::Picker;

use crate::SgRequest;

use super::{match_request::MatchRequest, SgHttpBackendService, SgRouteRuleService};

pub struct RouteByWeight;

impl<R> Picker<SgHttpBackendService, R> for RouteByWeight {
    fn pick(&mut self, _r: &R, services: &[SgHttpBackendService]) -> usize {
        let weights = services.iter().map(|x| x.weight);
        let Ok(weighted) = rand::distributions::WeightedIndex::new(weights) else { return 0 };
        weighted.sample(&mut rand::thread_rng())
    }
}

pub struct RouteByMatches;

impl Picker<SgRouteRuleService, SgRequest> for RouteByMatches {
    fn pick(&mut self, r: &SgRequest, services: &[SgRouteRuleService]) -> usize {
        for (i, service) in services.iter().enumerate() {
            if service.r#match.match_request(r) {
                return i;
            }
        }
        0
    }
}
