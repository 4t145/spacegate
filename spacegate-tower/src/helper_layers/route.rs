use std::ops::{Index, IndexMut};

use futures_util::future::BoxFuture;
pub use hyper::http::request::Parts;
use hyper::{Request, Response};
use tower::BoxError;
use tower_service::Service;

use crate::SgBody;

pub trait Router {
    type Index: Clone;
    fn route(&self, req: &Request<SgBody>) -> Option<Self::Index>;
}

pub struct Route<S, R, F>
where
    R: Router,
{
    services: S,
    fallback: F,
    router: R,
    unready_services: Vec<R::Index>,
}

impl<S, R, F> Route<S, R, F>
where
    R: Router,
{
    pub fn new(services: S, router: R, fallback: F) -> Self {
        Self {
            services,
            router,
            fallback,
            unready_services: Vec::new(),
        }
    }
}

impl<S, R, F> Service<Request<SgBody>> for Route<S, R, F>
where
    R: Router,
    S: IndexMut<R::Index>,
    S::Output: Service<Request<SgBody>, Response = Response<SgBody>, Error = BoxError> + Send + 'static,
    F: Service<Request<SgBody>, Response = Response<SgBody>, Error = BoxError> + Send + 'static,
    <F as Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
    <S::Output as Service<hyper::Request<SgBody>>>::Future: std::marker::Send,
{
    type Error = BoxError;
    type Response = Response<SgBody>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        while let Some(idx) = self.unready_services.pop() {
            let service = &mut self.services[idx.clone()];
            if let std::task::Poll::Ready(result) = service.poll_ready(cx) {
                result?;
                continue;
            } else {
                self.unready_services.push(idx);
                return std::task::Poll::Pending;
            }
        }
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        if let Some(index) = self.router.route(&req) {
            let fut = self.services.index_mut(index).call(req);
            Box::pin(fut)
        } else {
            let fut = self.fallback.call(req);
            Box::pin(fut)
        }
    }
}
