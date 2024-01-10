// pub mod config;
pub mod context;
pub mod helper_layers;
pub mod plugin_layers;
pub mod service;
pub mod utils;

use context::SgContext;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};

use hyper::{
    body::{Body, Bytes},
    Request, Response, StatusCode,
};
use tower::{
    util::{BoxLayer, BoxService},
    BoxError,
};
use utils::never;

#[derive(Debug)]
#[repr(transparent)]
pub struct SgBody(BoxBody<Bytes, hyper::Error>);

impl Body for SgBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let mut pinned = std::pin::pin!(&mut self.0);
        pinned.as_mut().poll_frame(cx)
    }
}

impl SgBody {
    pub fn empty() -> Self {
        Self(BoxBody::new(Empty::new().map_err(never)))
    }
    pub fn full(data: impl Into<Bytes>) -> Self {
        Self(BoxBody::new(Full::new(data.into()).map_err(never)))
    }
}

#[derive(Debug)]
pub struct SgRequest {
    pub context: SgContext,
    pub request: Request<SgBody>,
}

#[derive(Debug)]
pub struct SgResponse {
    pub context: SgContext,
    pub response: Response<SgBody>,
}

impl SgRequest {
    pub fn new(context: SgContext, request: Request<SgBody>) -> Self {
        Self { context, request }
    }
}

impl SgResponse {
    pub fn new(context: SgContext, response: Response<SgBody>) -> Self {
        Self { context, response }
    }
    pub fn with_code_message(context: SgContext, code: StatusCode, message: impl Into<Bytes>) -> Self {
        Self {
            context,
            response: Response::builder().status(code).body(SgBody::full(message)).expect("response builder error"),
        }
    }
}

pub type ReqOrResp = Result<SgRequest, SgResponse>;

type SgBoxService = BoxService<SgRequest, SgResponse, BoxError>;
type SgBoxLayer = BoxLayer<SgBoxService, SgRequest, SgResponse, BoxError>;
