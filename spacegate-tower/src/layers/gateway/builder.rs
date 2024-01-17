use crate::{layers::http_route::SgHttpRouter, SgBoxLayer, helper_layers::filter::{FilterRequestLayer, response_anyway::ResponseAnyway}};

pub struct SgGatewayLayerBuilder {
    routers: Vec<SgHttpRouter>,
    plugins: Vec<SgBoxLayer>,
    fallback: SgBoxLayer,
}

impl SgGatewayLayerBuilder {
    pub fn new() -> Self {
        Self {
            routers: Vec::new(),
            plugins: Vec::new(),
            fallback: SgBoxLayer::new(FilterRequestLayer::new(ResponseAnyway {
                status: hyper::StatusCode::NOT_FOUND,
                message: "[Sg.HttpRouteRule] no rule matched".to_string().into(),
            })),
        }
    }
    pub fn router(mut self, router: SgHttpRouter) -> Self {
        self.routers.push(router);
        self
    }
    pub fn plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.plugins.push(plugin);
        self
    }
    pub fn fallback(mut self, fallback: SgBoxLayer) -> Self {
        self.fallback = fallback;
        self
    }
    pub fn build(self) -> SgGatewayLayer {
        SgGatewayLayer {
            routers: self.routers.into(),
            plugins: self.plugins.into(),
            fallback: self.fallback,
        }
    }
}