use std::sync::Arc;

use hyper::{header::HeaderName, HeaderMap};
use tower::util::{MapRequestLayer, MapResponseLayer};
use tower_layer::Layer;
use hyper::{Request, Response};

use crate::{SgBoxService, SgBody,};

struct Filter {
    pub sets: HeaderMap,
    pub remove: Vec<HeaderName>,
}

pub struct HeaderModifierLayer {
    request: Arc<Filter>,
    response: Arc<Filter>,
}

impl Layer<SgBoxService> for HeaderModifierLayer {
    type Service = SgBoxService;

    fn layer(&self, service: SgBoxService) -> Self::Service {
        let req_filter = self.request.clone();
        let resp_filter = self.response.clone();
        let req_map_layer = MapRequestLayer::new(move |req: Request<SgBody>| {
            let mut req = req;
            for (k, v) in &req_filter.sets {
                req.headers_mut().append(k, v.clone());
            }
            for k in &req_filter.remove {
                req.headers_mut().remove(k);
            }
            req
        });
        let resp_map_layer = MapResponseLayer::new(move |resp: Response<SgBody>| {
            let mut resp = resp;
            for (k, v) in resp_filter.sets.iter() {
                resp.headers_mut().append(k, v.clone());
            }
            for k in &resp_filter.remove {
                resp.headers_mut().remove(k);
            }
            resp
        });
        SgBoxService::new(req_map_layer.layer(resp_map_layer.layer(service)))
    }
}
