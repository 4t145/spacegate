pub mod builder;

use std::{
    convert::Infallible,
    ops::{Index, IndexMut},
    sync::Arc,
    time::Duration,
};

use crate::{
    helper_layers::{
        filter::{FilterRequest, FilterRequestLayer},
        route::{Route, Router},
    },
    plugin_layers::MakeSgLayer,
    utils::fold_sg_layers::fold_sg_layers,
    SgBody, SgBoxLayer, SgBoxService,
};

use http_serde::authority;
use hyper::{Request, Response};
use tower::steer::Steer;

use tower_http::timeout::{Timeout, TimeoutLayer};

use tower_layer::Layer;
use tower_service::Service;

use super::http_route::{match_request::MatchRequest, SgHttpRoute, SgHttpRouter};

/****************************************************************************************

                                          Gateway

*****************************************************************************************/

pub struct SgGatewayLayer {
    http_routes: Arc<[SgHttpRoute]>,
    http_plugins: Arc<[SgBoxLayer]>,
    http_fallback: SgBoxLayer,
}

impl SgGatewayLayer {
    pub fn builder() -> builder::SgGatewayLayerBuilder {
        builder::SgGatewayLayerBuilder::new()
    }
}

#[derive(Debug, Clone)]
pub struct SgGatewayServices {
    services: Vec<Vec<SgBoxService>>,
}

#[derive(Debug, Clone)]
pub struct SgGatewayRouter {
    routers: Arc<[SgHttpRouter]>,
}

impl Index<(usize, usize)> for SgGatewayServices {
    type Output = SgBoxService;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.services[index.0][index.1]
    }
}

impl IndexMut<(usize, usize)> for SgGatewayServices {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        &mut self.services[index.0][index.1]
    }
}

impl Router for SgGatewayRouter {
    type Index = (usize, usize);

    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index> {
        for (idx0, route) in self.routers.iter().enumerate() {
            if let Some(host) = req.uri().host() {
                if route.hostnames.iter().any(|hostname| hostname == host) {
                    for (idx1, r#match) in route.rules.iter().enumerate() {
                        if r#match.match_request(req) {
                            return Some((idx0, idx1));
                        }
                    }
                }
            }
        }
        None
    }

    fn all_indexes(&self) -> std::collections::VecDeque<Self::Index> {
        let mut indexes = std::collections::VecDeque::new();
        for (idx0, route) in self.routers.iter().enumerate() {
            for (idx1, _) in route.rules.iter().enumerate() {
                indexes.push_back((idx0, idx1));
            }
        }
        indexes
    }
}

impl<S> Layer<S> for SgGatewayLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = Route<SgGatewayServices, SgGatewayRouter, SgBoxService>;

    fn layer(&self, inner: S) -> Self::Service {
        let gateway_plugins = self.http_plugins.iter().collect::<SgBoxLayer>();

        let mut services = Vec::with_capacity(self.http_routes.len());
        let mut routers = Vec::with_capacity(self.http_routes.len());
        for route in self.http_routes.iter() {
            let route_plugins = route.plugins.iter().collect::<SgBoxLayer>();
            let mut rules_services = Vec::with_capacity(route.rules.len());
            let mut rules_router = Vec::with_capacity(route.rules.len());
            for rule in route.rules.iter() {
                let rule_service = gateway_plugins.layer(route_plugins.layer(rule.layer(inner.clone())));
                rules_services.push(rule_service);
                rules_router.push(rule.r#match.clone());
            }
            services.push(rules_services);
            routers.push(SgHttpRouter {
                hostnames: route.hostnames.clone(),
                rules: rules_router.into(),
            });
        }
        Route::new(SgGatewayServices { services }, SgGatewayRouter { routers: routers.into() }, self.http_fallback.layer(inner))
    }
}
