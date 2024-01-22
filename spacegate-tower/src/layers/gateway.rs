pub mod builder;

use std::{
    convert::Infallible,
    ops::{Index, IndexMut},
    sync::Arc,
    time::Duration,
};

use crate::{
    extension::matched::Matched,
    helper_layers::{
        filter::{response_anyway::ResponseAnyway, FilterRequest, FilterRequestLayer},
        reload::Reloader,
        route::{Route, Router},
    },
    utils::fold_sg_layers::fold_sg_layers,
    SgBody, SgBoxLayer, SgBoxService,
};

use http_serde::authority;
use hyper::{
    header::{HeaderValue, HOST},
    Request, Response,
};
use tower::steer::Steer;

use tower_http::timeout::{Timeout, TimeoutLayer};

use tower_layer::Layer;
use tower_service::Service;
use tracing::instrument;

use super::http_route::{match_request::MatchRequest, SgHttpRoute, SgHttpRouter};

/****************************************************************************************

                                          Gateway

*****************************************************************************************/

pub type SgGatewayRoute = Route<SgGatewayRoutedServices, SgGatewayRouter, SgBoxService>;

pub struct SgGatewayLayer {
    http_routes: Arc<[SgHttpRoute]>,
    http_plugins: Arc<[SgBoxLayer]>,
    http_fallback: SgBoxLayer,
    pub http_route_reloader: Reloader<SgGatewayRoute>,
}

impl SgGatewayLayer {
    pub fn builder() -> builder::SgGatewayLayerBuilder {
        builder::SgGatewayLayerBuilder::new()
    }
}

#[derive(Debug, Clone)]
pub struct SgGatewayRoutedServices {
    services: Vec<Vec<SgBoxService>>,
}

#[derive(Debug, Clone)]
pub struct SgGatewayRouter {
    pub routers: Arc<[SgHttpRouter]>,
}

impl Index<(usize, usize)> for SgGatewayRoutedServices {
    type Output = SgBoxService;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.services[index.0][index.1]
    }
}

impl IndexMut<(usize, usize)> for SgGatewayRoutedServices {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        &mut self.services[index.0][index.1]
    }
}

// header: example.com:80 matches example.com
// header: example.com matches example.com
fn match_host(header: &[u8], matcher: &[u8]) -> bool {
    if header.len() < matcher.len() {
        return false;
    }
    let mut h_iter = header.iter();
    let mut m_iter = header.iter();
    loop {
        match (h_iter.next(), m_iter.next()) {
            (Some(h), Some(m)) => {
                if !h.eq_ignore_ascii_case(m) {
                    return false;
                }
            }
            (None, None) | (Some(b':'), None) => {
                return true;
            }
            _ => return false,
        }
    }
}

impl Router for SgGatewayRouter {
    type Index = (usize, usize);
    #[instrument(skip_all, fields(uri = req.uri().to_string(), method = req.method().as_str(), host = ?req.headers().get(HOST) ))]
    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index> {
        let host = req.headers().get(HOST).map(HeaderValue::as_bytes);
        for (idx0, route) in self.routers.iter().enumerate() {
            if route.hostnames.is_empty() || host.is_some_and(|host| route.hostnames.iter().any(|host_match| match_host(host, host_match.as_bytes()))) {
                for (idx1, r#match) in route.rules.iter().enumerate() {
                    if r#match.match_request(req) {
                        tracing::trace!("matches {match:?}");
                        return Some((idx0, idx1));
                    }
                }
            }
        }
        tracing::trace!("not matched");
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
    type Service = SgBoxService;

    fn layer(&self, inner: S) -> Self::Service {
        let gateway_plugins = self.http_plugins.iter().collect::<SgBoxLayer>();
        let route = create_http_router(&self.http_routes, &self.http_fallback, inner);
        let reloader = self.http_route_reloader.clone();
        let reload_service = reloader.into_layer().layer(route);
        gateway_plugins.layer(reload_service)
    }
}

pub fn create_http_router<S>(routes: &[SgHttpRoute], fallback: &SgBoxLayer, inner: S) -> Route<SgGatewayRoutedServices, SgGatewayRouter, SgBoxService>
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    let mut services = Vec::with_capacity(routes.len());
    let mut routers = Vec::with_capacity(routes.len());
    for route in routes {
        let route_plugins = route.plugins.iter().collect::<SgBoxLayer>();
        let mut rules_services = Vec::with_capacity(route.rules.len());
        let mut rules_router = Vec::with_capacity(route.rules.len());
        for rule in route.rules.iter() {
            let rule_service = route_plugins.layer(rule.layer(inner.clone()));
            rules_services.push(rule_service);
            rules_router.push(rule.r#match.clone());
        }
        services.push(rules_services);
        routers.push(SgHttpRouter {
            hostnames: route.hostnames.clone(),
            rules: rules_router.into(),
        });
    }
    Route::new(SgGatewayRoutedServices { services }, SgGatewayRouter { routers: routers.into() }, fallback.layer(inner))
}
