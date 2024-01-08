pub mod filters;

use std::{
    borrow::Cow,
    future::Future,
    ops::{Add, Mul},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use http_body_util::combinators::BoxBody;
use hyper::{
    body::{Body, Bytes},
    Request, Response,
};
use pin_project_lite::pin_project;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::{ready, FutureExt},
};
use tower_layer::Layer;
use tower_service::Service;

type SgBody = BoxBody<Bytes, TardisError>;
#[derive(Debug, Clone)]
pub struct SgRequest<B> {
    pub context: SgContext,
    pub request: Request<B>,
}

#[derive(Debug, Clone)]
pub struct SgResponse<B> {
    pub context: SgContext,
    pub response: Response<B>,
}

#[derive(Debug, Clone)]
pub struct SgContext {}

pub trait SgFilter<I, O>: Send + Sync {
    type FutureReq: Future<Output = Result<SgRequest<I>, SgResponse<O>>> + Send;
    type FutureResp: Future<Output = TardisResult<SgResponse<O>>> + Send;
    fn code(&self) -> Cow<'static, str>;
    fn on_create(&mut self) -> impl Future<Output = TardisResult<()>> + Send {
        async { Ok(()) }
    }
    fn on_destroy(&self) -> impl Future<Output = TardisResult<()>> + Send {
        async { Ok(()) }
    }
    fn on_req(&self, req: SgRequest<I>) -> Self::FutureReq;
    fn on_resp(&self, resp: SgResponse<O>) -> Self::FutureResp;
}

pub struct FilterLayer<F> {
    filter: F,
}

impl<F> FilterLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

pin_project! {
    pub struct FilterService<F, S> {
        #[pin]
        filter: F,
        service: S,
    }
}

impl<F, S> Layer<S> for FilterLayer<F>
where
    F: Clone,
{
    type Service = FilterService<F, S>;
    fn layer(&self, service: S) -> Self::Service {
        Self::Service {
            filter: self.filter.clone(),
            service,
        }
    }
}

impl<I, O, F, S> Service<SgRequest<I>> for FilterService<F, S>
where
    Self: Clone,
    S: Service<SgRequest<I>, Error = TardisError, Response = SgResponse<O>>,
    F: SgFilter<I, O>,
{
    type Response = SgResponse<O>;
    type Error = TardisError;
    type Future = FilterFuture<I, O, F, S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: SgRequest<I>) -> Self::Future {
        let cloned = self.clone();
        FilterFuture {
            request: Some(request),
            state: FilterFutureState::Start,
            filter: cloned,
        }
    }
}

pin_project! {
    pub struct FilterFuture<I, O, F, S>
    where
        S: Service<SgRequest<I>, Error = TardisError, Response = SgResponse<O>>,
        F: SgFilter<I, O>,
    {
        request: Option<SgRequest<I>>,
        #[pin]
        state: FilterFutureState<F::FutureReq, F::FutureResp, S::Future>,
        #[pin]
        filter: FilterService<F, S>,
    }
}

pin_project! {
    #[project = FilterFutureStateProj]
    pub enum FilterFutureState<FReq, FResp, S> {
        Start,
        Request {
            #[pin]
            fut: FReq,
        },
        InnerCall {
            #[pin]
            fut: S,
        },
        Response {
            #[pin]
            fut: FResp,
        },
    }
}

impl<I, O, F, S> Future for FilterFuture<I, O, F, S>
where
    S: Service<SgRequest<I>, Error = TardisError, Response = SgResponse<O>>,
    F: SgFilter<I, O>,
{
    type Output = TardisResult<SgResponse<O>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.state.as_mut().project() {
                FilterFutureStateProj::Start => {
                    let fut = this.filter.filter.on_req(this.request.take().expect("missing request at start state"));
                    this.state.set(FilterFutureState::Request { fut });
                }
                FilterFutureStateProj::Request { fut } => {
                    let request_result = ready!(fut.poll(cx));
                    match request_result {
                        Ok(req) => {
                            let fut = this.filter.as_mut().project().service.call(req);
                            this.state.set(FilterFutureState::InnerCall { fut });
                        }
                        Err(resp) => {
                            return Poll::Ready(Ok(resp));
                        }
                    }
                }
                FilterFutureStateProj::InnerCall { fut } => {
                    let request_result = ready!(fut.poll(cx))?;
                    let fut = this.filter.filter.on_resp(request_result);
                    this.state.set(FilterFutureState::Response { fut });
                }
                FilterFutureStateProj::Response { fut } => {
                    let request_result = ready!(fut.poll(cx))?;
                    return Poll::Ready(Ok(request_result));
                }
            }
        }
    }
}
