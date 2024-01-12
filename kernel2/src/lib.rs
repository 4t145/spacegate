// pub mod config;
pub mod body;
pub mod context;
pub mod helper_layers;
pub mod plugin_layers;
pub mod route_layers;
pub mod utils;

use std::{convert::Infallible, fmt, sync::Arc};

use context::SgContext;
use helper_layers::response_error::ErrorFormatter;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};

use hyper::{
    body::{Body, Bytes},
    Request, Response, StatusCode,
};
use tower::util::BoxCloneService;
use tower_layer::{layer_fn, Layer};
use tower_service::Service;
use utils::{fold_sg_layers::fold_sg_layers, never};

#[derive(Debug)]
pub struct SgBody {
    body: BoxBody<Bytes, hyper::Error>,
    context: SgContext,
}

impl Default for SgBody {
    fn default() -> Self {
        Self::empty()
    }
}

impl Body for SgBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let mut pinned = std::pin::pin!(&mut self.body);
        pinned.as_mut().poll_frame(cx)
    }
}

impl SgBody {
    pub fn new(body: impl Body<Data = Bytes, Error = hyper::Error> + Send + Sync + 'static) -> Self {
        Self {
            body: BoxBody::new(body),
            context: SgContext::default(),
        }
    }
    pub fn empty() -> Self {
        Self {
            body: BoxBody::new(Empty::new().map_err(never)),
            context: SgContext::default(),
        }
    }
    pub fn full(data: impl Into<Bytes>) -> Self {
        Self {
            body: BoxBody::new(Full::new(data.into()).map_err(never)),
            context: SgContext::default(),
        }
    }
}

pub trait SgResponseExt {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self;
    fn internal_error<E: std::error::Error>(e: E) -> Self
    where
        Self: Sized,
    {
        Self::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
    fn from_error<E: std::error::Error, F: ErrorFormatter>(e: E, formatter: &F) -> Self
    where
        Self: Sized,
    {
        Self::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, formatter.format(&e))
    }
}

impl SgResponseExt for Response<SgBody> {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self {
        let body = SgBody::full(message);
        Response::builder().status(code).body(body).expect("response builder error")
    }
}

pub type ReqOrResp = Result<Request<SgBody>, Response<SgBody>>;

type SgBoxService = BoxCloneService<Request<SgBody>, Response<SgBody>, Infallible>;
// type SgBoxLayer<S> = BoxLayer<S, Request<SgBody>, Response<SgBody>, Infallible>;

pub struct SgBoxLayer {
    boxed: Arc<dyn Layer<SgBoxService, Service = SgBoxService> + Send + Sync + 'static>,
}

impl FromIterator<SgBoxLayer> for SgBoxLayer {
    fn from_iter<T: IntoIterator<Item = SgBoxLayer>>(iter: T) -> Self {
        fold_sg_layers(iter.into_iter())
    }
}

impl<'a> FromIterator<&'a SgBoxLayer> for SgBoxLayer {
    fn from_iter<T: IntoIterator<Item = &'a SgBoxLayer>>(iter: T) -> Self {
        fold_sg_layers(iter.into_iter().cloned())
    }
}

impl SgBoxLayer {
    /// Create a new [`BoxLayer`].
    pub fn new<L>(inner_layer: L) -> Self
    where
        L: Layer<SgBoxService> + Send + Sync + 'static,
        L::Service: Clone + Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
        <L::Service as Service<Request<SgBody>>>::Future: Send + 'static,
    {
        let layer = layer_fn(move |inner: SgBoxService| {
            let out = inner_layer.layer(inner);
            SgBoxService::new(out)
        });

        Self { boxed: Arc::new(layer) }
    }
}

impl<S> Layer<S> for SgBoxLayer
where
    S: Clone + Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible> + Send + 'static,
    <S as tower_service::Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Service = SgBoxService;

    fn layer(&self, inner: S) -> Self::Service {
        self.boxed.layer(SgBoxService::new(inner))
    }
}

impl Clone for SgBoxLayer {
    fn clone(&self) -> Self {
        Self { boxed: Arc::clone(&self.boxed) }
    }
}

impl fmt::Debug for SgBoxLayer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BoxLayer").finish()
    }
}
