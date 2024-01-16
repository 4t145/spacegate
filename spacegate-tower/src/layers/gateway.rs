use std::sync::Arc;

use hyper::Request;

use crate::{SgBoxLayer, SgBody};

use super::{
    http_route::{SgHttpRoute, SgHttpRouteLayer},
    route::{Route, Router},
};

pub struct GatewayLayer {
    routes: Arc<[SgHttpRouteLayer]>,
    filters: Arc<[SgBoxLayer]>,
}

pub struct Gateway {
    routes: Vec<SgHttpRoute>,
}

impl Router for Vec<SgHttpRoute> {
    type Index = usize;

    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index> {
        for (idx, route) in self.iter().enumerate() {
            if route.hostnames.iter().any(|hostname| hostname == req.uri().host().unwrap()) {
                return Some(idx);
            }
        }
        None
    }
}
