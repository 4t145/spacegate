mod builder;
pub mod match_request;
mod picker;
mod predicate;
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::{
    helper_layers::filter::{FilterRequest, FilterRequestLayer},
    plugin_layers::MakeSgLayer,
    utils::fold_sg_layers::fold_sg_layers,
    SgBody, SgBoxLayer, SgBoxService,
};

use hyper::{Request, Response};
use tower::steer::Steer;

use tower_http::timeout::{Timeout, TimeoutLayer};

use tower_layer::Layer;
use tower_service::Service;

use self::{
    builder::{SgHttpBackendLayerBuilder, SgHttpRouteLayerBuilder, SgHttpRouteRuleLayerBuilder},
    match_request::SgHttpRouteMatch,
    picker::{RouteByMatches, RouteByWeight},
    predicate::FilterByHostnames,
};

/****************************************************************************************

                                          Route

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpRouteLayer {
    pub hostnames: Arc<[String]>,
    pub rules: Arc<[SgHttpRouteRuleLayer]>,
    pub fallback_index: usize,
}

impl SgHttpRouteLayer {
    pub fn builder() -> SgHttpRouteLayerBuilder {
        SgHttpRouteLayerBuilder::new()
    }
}
#[derive(Clone)]
pub struct SgHttpRoute {
    pub hostnames: Arc<[String]>,
    inner: FilterRequest<FilterByHostnames, Steer<SgRouteRule, RouteByMatches, Request<SgBody>>>,
}

impl<S> Layer<S> for SgHttpRouteLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgHttpRoute;

    fn layer(&self, inner: S) -> Self::Service {
        let steer = <Steer<_, _, Request<SgBody>>>::new(
            self.rules.iter().map(|l| l.layer(inner.clone())),
            RouteByMatches {
                fallback_index: self.fallback_index,
            },
        );
        let filtered = FilterRequestLayer::new(FilterByHostnames {
            hostnames: self.hostnames.clone(),
        })
        .layer(steer);
        SgHttpRoute {
            hostnames: self.hostnames.clone(),
            inner: filtered,
        }
    }
}

impl Service<Request<SgBody>> for SgHttpRoute {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        Box::pin(self.inner.call(req))
    }

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
}

/****************************************************************************************

                                        Route Rule

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpRouteRuleLayer {
    r#match: Arc<Option<SgHttpRouteMatch>>,
    filters: Arc<[SgBoxLayer]>,
    timeouts: Option<Duration>,
    backends: Arc<[SgHttpBackendLayer]>,
}

impl SgHttpRouteRuleLayer {
    pub fn builder() -> SgHttpRouteRuleLayerBuilder {
        SgHttpRouteRuleLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpRouteRuleLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgRouteRule;

    fn layer(&self, inner: S) -> Self::Service {
        let steer = <Steer<_, _, Request<SgBody>>>::new(self.backends.iter().map(|l| l.layer(inner.clone())), RouteByWeight);
        let filter_layer = self.filters.iter().collect::<SgBoxLayer>();
        let service = if let Some(timeout) = self.timeouts {
            SgBoxService::new(TimeoutLayer::new(timeout).layer(filter_layer.layer(steer)))
        } else {
            SgBoxService::new(filter_layer.layer(SgBoxService::new(steer)))
        };
        SgRouteRule {
            r#match: self.r#match.clone(),
            service,
        }
    }
}
#[derive(Clone)]
pub struct SgRouteRule {
    r#match: Arc<Option<SgHttpRouteMatch>>,
    service: SgBoxService,
}

impl Service<Request<SgBody>> for SgRouteRule {
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;
    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        self.service.call(req)
    }
}

/****************************************************************************************

                                        Backend

*****************************************************************************************/

#[derive(Debug, Clone)]
pub struct SgHttpBackendLayer {
    pub filters: Arc<[SgBoxLayer]>,
    pub weight: u16,
    pub timeout: Option<Duration>,
}

impl SgHttpBackendLayer {
    pub fn builder() -> SgHttpBackendLayerBuilder {
        SgHttpBackendLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpBackendLayer
where
    S: Clone + Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Send + Sync + 'static,
    <S as tower_service::Service<Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgHttpBackend<SgBoxService>;

    fn layer(&self, inner: S) -> Self::Service {
        let mut filtered = self.filters.iter().collect::<SgBoxLayer>().layer(SgBoxService::new(inner));
        if let Some(timeout) = self.timeout {
            filtered = SgBoxService::new(Timeout::new(filtered, timeout));
        }
        SgHttpBackend {
            weight: self.weight,
            inner_service: SgBoxService::new(filtered),
        }
    }
}

#[derive(Clone)]
pub struct SgHttpBackend<S> {
    pub weight: u16,
    pub inner_service: S,
}

impl<S> Service<Request<SgBody>> for SgHttpBackend<S>
where
    S: Clone + Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <S as Service<Request<SgBody>>>::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        Box::pin(self.inner_service.call(req))
    }

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner_service.poll_ready(cx)
    }
}
