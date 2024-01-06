use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use http_body_util::combinators::BoxBody;
use hyper::{
    body::{Body, Bytes},
    Request, Response,
};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::FutureExt,
};
use tower_layer::Layer;
use tower_service::Service;

type SgBody = BoxBody<Bytes, TardisError>;
pub struct SgRequest {
    pub context: SgContext,
    pub request: Request<SgBody>,
}

pub struct SgResponse {
    pub context: SgContext,
    pub response: Response<SgBody>,
}

pub struct SgContext {}

pub enum SgPluginFilterPayload {
    Request(SgRequest),
    Response(SgResponse),
}
pub trait SgFilter: Send + Sync + 'static {
    fn on_req(&self, req: SgRequest) -> impl Future<Output = TardisResult<SgPluginFilterPayload>> + Send;

    fn on_resp(&self, resp: SgResponse) -> impl Future<Output = TardisResult<SgPluginFilterPayload>> + Send;
}

pub struct Concat<F1, F2>(F1, F2);

impl<F1, F2> SgFilter for (F1, F2)
where
    F1: SgFilter,
    F2: SgFilter,
{
    fn on_req(&self, req: SgRequest) -> impl Future<Output = TardisResult<SgPluginFilterPayload>> + Send {
        let (f1, f2) = self;
        let f1 = f1.on_req(req);
        async move {
            let payload = f1.await?;
            match payload {
                SgPluginFilterPayload::Request(req) => f2.on_req(req).await,
                SgPluginFilterPayload::Response(resp) => Ok(SgPluginFilterPayload::Response(resp)),
            }
        }
    }

    fn on_resp(&self, resp: SgResponse) -> impl Future<Output = TardisResult<SgPluginFilterPayload>> + Send {
        let (f1, f2) = self;
        let f1 = f1.on_resp(resp);
        async move {
            let payload = f1.await?;
            match payload {
                SgPluginFilterPayload::Request(req) => f2.on_req(req).await,
                SgPluginFilterPayload::Response(resp) => Ok(SgPluginFilterPayload::Response(resp)),
            }
        }
    }
}

pub struct SgPluginFilterService<F, S> {
    pub filter: F,
    pub service: S,
}

enum State {
    Init,
    OnReq,
    InnerLayer,
    OnResp,
}

impl<S, F> Service<SgRequest> for SgPluginFilterService<F, S>
where
    S: Service<SgRequest, Error = TardisError, Response = SgResponse>,
    F: SgFilter,
{
    type Error = TardisError;
    type Response = SgResponse;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }
    fn call(&mut self, req: SgRequest) -> Self::Future {
        let req = self.filter.on_req(req);
    }
}

use pin_project_lite::pin_project;
struct FilterFuture {
    state: FilterFutureState,
}

enum FilterFutureState {
    ReqPoll,
    ReqFinished(TardisResult<SgPluginFilterPayload>),
    InnerPoll,
    Inner(TardisResult<SgResponse>),
    RespPoll,
    Resp(TardisResult<SgPluginFilterPayload>),
}


impl Future for FilterFuture {
    
}