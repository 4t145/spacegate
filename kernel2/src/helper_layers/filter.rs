use std::{
    convert::Infallible,
    future::{Future, Ready},
    task::ready,
};

use crate::SgBody;
use hyper::{Request, Response};
use pin_project_lite::pin_project;
use tower_layer::Layer;
use tower_service::Service;

pub trait Filter: Clone {
    fn filter(&self, req: Request<SgBody>) -> Result<Request<SgBody>, Response<SgBody>>;
}


pub struct FilterRequestLayer<F> {
    filter: F,
}

impl<F> FilterRequestLayer<F> {
    pub fn new(filter: F) -> Self {
        Self { filter }
    }
}

impl<F, S> Layer<S> for FilterRequestLayer<F>
where F: Filter
{
    type Service = FilterRequest<F, S>;

    fn layer(&self, inner: S) -> Self::Service {
        FilterRequest {
            filter: self.filter.clone(),
            inner,
        }
    }
}

#[derive(Clone)]
pub struct FilterRequest<F, S> {
    filter: F,
    inner: S,
}

// pin_project! {
//     #[project = FilterResponseFutureStateProj]
//     enum FilterResponseFutureState<F, S> {
//         Filter {
//             #[pin]
//             fut: F
//         },
//         Inner {
//             #[pin]
//             fut: S
//         },
//     }
// }

// pin_project! {
//     pub struct FilterResponseFuture<F, S, SFut> {
//         #[pin]
//         state: FilterResponseFutureState<F, SFut>
//         inner_service: S
//     }
// }

// impl<F, S, SFut> FilterResponseFuture<F, S, SFut> {
//     // pub fn
// }

// impl<F, S> Future for FilterResponseFuture<F, S>
// where
//     F: Future<Output = Result<Request<SgBody>, Response<SgBody>>>,
//     S: Future<Output = Result<Response<SgBody>, Infallible>>,
// {
//     type Output = Result<Response<SgBody>, Infallible>;

//     fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
//         loop {
//             let mut this = self.project();
//             match this.state.project() {
//                 FilterResponseFutureStateProj::Filter { fut } => match ready!(fut.poll(cx)) {
//                     Ok(req) => *this.state = FilterResponseFutureState::Inner { fut: this.inner.call(req) },
//                     Err(resp) => return std::task::Poll::Ready(Ok(resp)),
//                 },
//                 FilterResponseFutureStateProj::Inner { fut } => {
//                     let resp = ready!(fut.poll(cx)).expect("infallible");
//                     return std::task::Poll::Ready(Ok(resp));
//                 }
//             }
//         }
//     }
// }

impl<F, S> Service<Request<SgBody>> for FilterRequest<F, S>
where
    F: Filter,
    S: Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = futures_util::future::Either<Ready<Result<Self::Response, Self::Error>>, S::Future>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        match self.filter.filter(req) {
            Ok(req) => futures_util::future::Either::Right(self.inner.call(req)),
            Err(resp) => futures_util::future::Either::Left(std::future::ready(Ok(resp))),
        }
    }
}

