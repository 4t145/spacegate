use hyper::{Request, Response};
use pin_project_lite::pin_project;
use tower::BoxError;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll}, convert::Infallible,
};
use tardis::{
    basic::{result::TardisResult},
};
use futures_util::ready;
use tower_layer::Layer;
use tower_service::Service;
pub use tower::util::{MapRequest, MapResponse, MapFuture};
use crate::{SgBoxService, SgBody};

/// Bi-Direction Filter
pub trait Bdf: Send + Sync {
    type FutureReq: Future<Output = Result<Request<SgBody>, Response<SgBody>>> + Send;
    type FutureResp: Future<Output = Response<SgBody>> + Send;

    fn on_req(&self, req: Request<SgBody>) -> Self::FutureReq;
    fn on_resp(&self, resp: Response<SgBody>) -> Self::FutureResp;
}

/// Bi-Direction Filter Layer
pub struct BdfLayer<F> {
    filter: F,
}

impl<F> BdfLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

pin_project! {
    pub struct BdfService<F, S> {
        #[pin]
        filter: F,
        service: S,
    }
}

impl<F> Layer<SgBoxService> for BdfLayer<F>
where
    F: Clone,
{
    type Service = BdfService<F, SgBoxService>;
    fn layer(&self, service: SgBoxService) -> Self::Service {
        Self::Service {
            filter: self.filter.clone(),
            service,
        }
    }
}

impl<F, S> Service<Request<SgBody>> for BdfService<F, S>
where
    Self: Clone,
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
    F: Bdf,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = FilterFuture<F, S>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<SgBody>) -> Self::Future {
        let cloned = self.clone();
        FilterFuture {
            request: Some(request),
            state: FilterFutureState::Start,
            filter: cloned,
        }
    }
}

pin_project! {
    pub struct FilterFuture<F, S>
    where
        S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
        F: Bdf,
    {
        request: Option<Request<SgBody>>,
        #[pin]
        state: FilterFutureState<F::FutureReq, F::FutureResp, S::Future>,
        #[pin]
        filter: BdfService<F, S>,
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

impl<F, S> Future for FilterFuture<F, S>
where
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
    F: Bdf,
{
    type Output = Result<Response<SgBody>, Infallible>;

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
                    let request_result = ready!(fut.poll(cx));
                    return Poll::Ready(Ok(request_result));
                }
            }
        }
    }
}
