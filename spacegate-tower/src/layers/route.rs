use std::ops::{Index, IndexMut};

use futures_util::future::BoxFuture;
pub use hyper::http::request::Parts;
use hyper::{Request, Response};
use tower::BoxError;
use tower_service::Service;

use crate::SgBody;

pub trait Router {
    type Index;
    fn route<B>(&self, req: &Request<B>) -> Option<Self::Index>;
}

pub trait Services<Idx> {}

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
    F: Service<Request<SgBody>, Response = Response<SgBody>, Error = BoxError> + Send + 'static,
{
    type Error = BoxError;
    type Response = Response<SgBody>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        let mut new_unready = Vec::new();
        for idx in self.unready_services.drain(..) {
            let service = &mut self.services[idx];
            if let std::task::Poll::Ready(Ok(())) = service.poll_ready(cx) {
                continue;
            } else {
                new_unready.push(idx);
            }
            self.unready_services.push(idx);
        }
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        todo!()
    }
}
