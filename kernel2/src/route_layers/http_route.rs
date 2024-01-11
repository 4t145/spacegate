mod builder;
pub mod match_request;
mod picker;
mod predicate;
use std::{sync::Arc, time::Duration};

use crate::{helper_layers::imresp_layer, plugin_layers::MakeSgLayer, utils::fold_sg_layers::fold_sg_layers, SgBoxLayer, SgBoxService, SgRequest, SgResponse};

use tower::{
    filter::FilterLayer,
    steer::Steer,
    timeout::{Timeout, TimeoutLayer},
    BoxError,
};
use tower_layer::Layer;
use tower_service::Service;

use self::{
    builder::{SgHttpBackendLayerBuilder, SgHttpRouteRuleLayerBuilder, SgHttpRouteLayerBuilder},
    match_request::SgHttpRouteMatch,
    picker::{RouteByMatches, RouteByWeight},
    predicate::FilterByHostnames,
};

/****************************************************************************************

                                          Route

*****************************************************************************************/
pub struct SgHttpRouteLayer {
    pub hostnames: Arc<[String]>,
    pub rules: Arc<[SgHttpRouteRuleLayer]>,
}

impl SgHttpRouteLayer {
    pub fn builder() -> SgHttpRouteLayerBuilder {
        SgHttpRouteLayerBuilder::new()
    }
}

impl<S> Layer<S> for SgHttpRouteLayer
where
    S: Clone + Service<SgRequest, Error = BoxError, Response = SgResponse> + Send + Sync + 'static,
    <S as tower_service::Service<SgRequest>>::Future: std::marker::Send,
{
    type Service = SgBoxService;

    fn layer(&self, inner: S) -> Self::Service {
        let filter_layer = FilterLayer::new(FilterByHostnames {
            hostnames: self.hostnames.clone(),
        });
        let steer = <Steer<_, _, SgRequest>>::new(self.rules.iter().map(|l| l.layer(inner.clone())), RouteByMatches);
        let filtered = filter_layer.layer(imresp_layer::ImmediatelyResponseLayer.layer(SgBoxService::new(steer)));
        SgBoxService::new(filtered)
    }
}

/****************************************************************************************

                                        Route Rule

*****************************************************************************************/

#[derive(Clone)]
pub struct SgHttpRouteRuleLayer {
    r#match: Arc<SgHttpRouteMatch>,
    filters: Arc<[SgBoxLayer]>,
    timeouts: Option<Duration>,
    backends: Arc<[SgHttpBackendLayer]>,
}

impl SgHttpRouteRuleLayer {
    pub fn builder(r#match: SgHttpRouteMatch) -> SgHttpRouteRuleLayerBuilder {
        SgHttpRouteRuleLayerBuilder::new(r#match)
    }
}

impl<S> Layer<S> for SgHttpRouteRuleLayer
where
    S: Clone + Service<SgRequest, Error = BoxError, Response = SgResponse> + Send + Sync + 'static,
    <S as tower_service::Service<SgRequest>>::Future: std::marker::Send,
{
    type Service = SgRouteRuleService;

    fn layer(&self, inner: S) -> Self::Service {
        let steer = <Steer<_, _, SgRequest>>::new(self.backends.iter().map(|l| l.layer(inner.clone())), RouteByWeight);
        let filter_layer = fold_sg_layers(self.filters.iter().cloned());
        let service = if let Some(timeout) = self.timeouts {
            SgBoxService::new(TimeoutLayer::new(timeout).layer(filter_layer.layer(SgBoxService::new(steer))))
        } else {
            SgBoxService::new(filter_layer.layer(SgBoxService::new(steer)))
        };
        SgRouteRuleService {
            r#match: self.r#match.clone(),
            service,
        }
    }
}

pub struct SgRouteRuleService {
    r#match: Arc<SgHttpRouteMatch>,
    service: SgBoxService,
}

impl Service<SgRequest> for SgRouteRuleService {
    type Response = SgResponse;
    type Error = BoxError;
    type Future = <SgBoxService as Service<SgRequest>>::Future;
    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: SgRequest) -> Self::Future {
        self.service.call(req)
    }
}

/****************************************************************************************

                                        Backend

*****************************************************************************************/

pub struct SgHttpBackendLayer {
    pub weight: u16,
    pub timeout: Option<Duration>,
    pub client: SgBoxLayer,
}

impl SgHttpBackendLayer {
    pub fn builder(client: impl MakeSgLayer) -> SgHttpBackendLayerBuilder {
        SgHttpBackendLayerBuilder::new(client)
    }
}

impl<S> Layer<S> for SgHttpBackendLayer
where
    S: Service<SgRequest, Error = BoxError, Response = SgResponse> + Send + Sync + 'static,
    <S as tower_service::Service<SgRequest>>::Future: std::marker::Send,
{
    type Service = SgHttpBackendService;

    fn layer(&self, inner: S) -> Self::Service {
        let mut service = self.client.layer(SgBoxService::new(inner));
        if let Some(timeout) = self.timeout {
            service = SgBoxService::new(Timeout::new(service, timeout));
        }
        SgHttpBackendService {
            weight: self.weight,
            inner_service: service,
        }
    }
}

pub struct SgHttpBackendService {
    pub weight: u16,
    pub inner_service: SgBoxService,
}

impl Service<SgRequest> for SgHttpBackendService {
    type Response = SgResponse;
    type Error = BoxError;
    type Future = <SgBoxService as Service<SgRequest>>::Future;

    fn call(&mut self, req: SgRequest) -> Self::Future {
        self.inner_service.call(req)
    }

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner_service.poll_ready(cx)
    }
}
