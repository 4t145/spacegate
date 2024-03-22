use std::{collections::HashMap, sync::Arc};

use hyper::header::HeaderValue;
use hyper::{header::HeaderName, HeaderMap};
use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_kernel::helper_layers::{map_request::MapRequestLayer, map_response::MapResponseLayer};
use spacegate_kernel::service::BoxHyperService;
use tower_layer::Layer;

use spacegate_kernel::SgBody;

use crate::{def_plugin, MakeSgLayer};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SgFilterHeaderModifierKind {
    #[default]
    Request,
    Response,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]

pub struct SgFilterHeaderModifier {
    pub kind: SgFilterHeaderModifierKind,
    pub sets: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

impl MakeSgLayer for SgFilterHeaderModifier {
    fn make_layer(&self) -> Result<spacegate_kernel::SgBoxLayer, spacegate_kernel::BoxError> {
        let mut sets = HeaderMap::new();
        if let Some(set) = &self.sets {
            for (k, v) in set.iter() {
                sets.insert(HeaderName::from_bytes(k.as_bytes())?, HeaderValue::from_bytes(v.as_bytes())?);
            }
        }
        let mut remove = Vec::new();
        if let Some(r) = &self.remove {
            for k in r {
                remove.push(k.parse()?);
            }
        }
        let filter = Filter { sets, remove };
        let layer = match self.kind {
            SgFilterHeaderModifierKind::Request => HeaderModifierLayer {
                request: Arc::new(filter),
                response: Arc::new(Filter::default()),
            },
            SgFilterHeaderModifierKind::Response => HeaderModifierLayer {
                request: Arc::new(Filter::default()),
                response: Arc::new(filter),
            },
        };
        Ok(spacegate_kernel::SgBoxLayer::new(layer))
    }
}

#[derive(Clone, Default, Debug)]
struct Filter {
    pub sets: HeaderMap,
    pub remove: Vec<HeaderName>,
}

pub struct HeaderModifierLayer {
    request: Arc<Filter>,
    response: Arc<Filter>,
}

impl Layer<BoxHyperService> for HeaderModifierLayer {
    type Service = BoxHyperService;

    fn layer(&self, service: BoxHyperService) -> Self::Service {
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
        BoxHyperService::new(req_map_layer.layer(resp_map_layer.layer(service)))
    }
}

def_plugin!("header_modifier", HeaderModifierPlugin, SgFilterHeaderModifier);
#[cfg(feature = "schema")]
crate::schema!(
    HeaderModifierPlugin
);
