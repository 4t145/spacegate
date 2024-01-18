use crate::{
    helper_layers::filter::{response_anyway::ResponseAnyway, FilterRequestLayer},
    layers::http_route::{SgHttpRoute, SgHttpRouter},
    SgBoxLayer,
};

use super::SgGatewayLayer;

pub struct SgGatewayLayerBuilder {
    http_routers: Vec<SgHttpRoute>,
    http_plugins: Vec<SgBoxLayer>,
    http_fallback: SgBoxLayer,
}

impl Default for SgGatewayLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgGatewayLayerBuilder {
    pub fn new() -> Self {
        Self {
            http_routers: Vec::new(),
            http_plugins: Vec::new(),
            http_fallback: SgBoxLayer::new(FilterRequestLayer::new(ResponseAnyway {
                status: hyper::StatusCode::NOT_FOUND,
                message: "[Sg.HttpRouteRule] no rule matched".to_string().into(),
            })),
        }
    }
    pub fn http_router(mut self, route: SgHttpRoute) -> Self {
        self.http_routers.push(route);
        self
    }
    pub fn http_routers(mut self, routes: impl IntoIterator<Item = SgHttpRoute>) -> Self {
        self.http_routers.extend(routes);
        self
    }
    pub fn http_plugin(mut self, plugin: SgBoxLayer) -> Self {
        self.http_plugins.push(plugin);
        self
    }
    pub fn http_plugins(mut self, plugins: impl IntoIterator<Item = SgBoxLayer>) -> Self {
        self.http_plugins.extend(plugins);
        self
    }
    pub fn http_fallback(mut self, fallback: SgBoxLayer) -> Self {
        self.http_fallback = fallback;
        self
    }
    pub fn build(self) -> SgGatewayLayer {
        SgGatewayLayer {
            http_routes: self.http_routers.into(),
            http_plugins: self.http_plugins.into(),
            http_fallback: self.http_fallback,
        }
    }
}
