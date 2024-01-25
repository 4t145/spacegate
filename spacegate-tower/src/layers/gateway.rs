pub mod builder;

use std::{
    convert::Infallible,
    ops::{Index, IndexMut},
    sync::Arc,
};

use crate::{
    helper_layers::{
        reload::Reloader,
        route::{Route, Router},
    },
    SgBody, SgBoxLayer, SgBoxService,
};

use hyper::{header::HOST, Request, Response};
use tokio_util::sync::CancellationToken;

use tower_layer::Layer;
use tower_service::Service;
use tracing::instrument;

use super::http_route::{match_hostname::HostnameTree, match_request::MatchRequest, SgHttpRoute, SgHttpRouter};

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
    /// Create a new gateway layer.
    /// # Arguments
    /// * `gateway_name` - The gateway name, this may be used by plugins.
    /// * `cancel_token` - A cancel token hints wether the gateway server is still alive.
    ///
    pub fn builder(gateway_name: impl Into<Arc<str>>, cancel_token: CancellationToken) -> builder::SgGatewayLayerBuilder {
        builder::SgGatewayLayerBuilder::new(gateway_name, cancel_token)
    }
}

#[derive(Debug, Clone)]
pub struct SgGatewayRoutedServices {
    services: Vec<Vec<SgBoxService>>,
}

#[derive(Debug, Clone)]
pub struct SgGatewayRouter {
    pub routers: Arc<[SgHttpRouter]>,
    pub hostname_tree: Arc<HostnameTree<usize>>,
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

impl Router for SgGatewayRouter {
    type Index = (usize, usize);
    #[instrument(skip_all, fields(uri = req.uri().to_string(), method = req.method().as_str(), host = ?req.headers().get(HOST) ))]
    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index> {
        let host = req.headers().get(HOST).and_then(|x| x.to_str().ok())?;
        let idx0 = *self.hostname_tree.get(host)?;
        for (idx1, r#match) in self.routers.as_ref().index(idx0).rules.iter().enumerate() {
            if r#match.match_request(req) {
                tracing::trace!("matches {match:?}");
                return Some((idx0, idx1));
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
    let mut hostname_tree = HostnameTree::new();
    for route in routes {
        for hostname in route.hostnames.iter() {
            hostname_tree.set(hostname, services.len());
        }
        let route_plugins = route.plugins.iter().collect::<SgBoxLayer>();
        let mut rules_services = Vec::with_capacity(route.rules.len());
        let mut rules_router = Vec::with_capacity(route.rules.len());
        for rule in route.rules.iter() {
            let rule_service = route_plugins.layer(rule.layer(inner.clone()));
            rules_services.push(rule_service);
            rules_router.push(rule.r#match.clone());
        }
        let idx = services.len();
        if route.hostnames.is_empty() {
            hostname_tree.set("*", idx);
        } else {
            for hostname in route.hostnames.iter() {
                hostname_tree.set(hostname, idx);
            }
        }
        services.push(rules_services);
        routers.push(SgHttpRouter {
            hostnames: route.hostnames.clone(),
            rules: rules_router.into(),
        });
    }
    Route::new(
        SgGatewayRoutedServices { services },
        SgGatewayRouter {
            routers: routers.into(),
            hostname_tree: Arc::new(hostname_tree),
        },
        fallback.layer(inner),
    )
}
