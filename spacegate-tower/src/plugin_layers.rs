use std::convert::Infallible;

use hyper::{Request, Response};
use tower::BoxError;
use tower_layer::Layer;
use tower_service::Service;

use crate::{SgBody, SgBoxLayer, SgBoxService};

pub mod header_modifier;
pub mod inject;
pub mod rate_limit;
pub mod redirect;
pub mod retry;

// pub mod comde;

pub trait MakeSgLayer {
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError>;
}

#[derive(Debug, Clone)]
pub struct SgLayer<L>(pub L);

impl<L> MakeSgLayer for SgLayer<L>
where
    L: Layer<SgBoxService> + Send + Sync + 'static + Clone,
    L::Service: Clone + Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <L::Service as Service<Request<SgBody>>>::Future: Send + 'static,
{
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError> {
        Ok(SgBoxLayer::new(self.0.clone()))
    }
}
