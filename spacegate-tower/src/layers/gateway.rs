use std::sync::Arc;

use crate::SgBoxLayer;

use super::http_route::{SgHttpRoute, SgHttpRouteLayer};

pub struct GatewayLayer {
    routes: Arc<[SgHttpRouteLayer]>,
    filters: Arc<[SgBoxLayer]>,
}

pub struct Gateway {
    routes: Arc<[SgHttpRoute]>,
    filters: Arc<[SgBoxLayer]>,
}
