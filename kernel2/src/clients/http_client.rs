use std::{convert::Infallible, pin::Pin, sync::Arc};

use crate::{
    helper_layers::response_error::{DefaultErrorFormatter, ResponseError, ResponseErrorFuture},
    SgBody, SgRequestExt, plugin_layers::MakeSgLayer,
};
use futures_util::{Future, FutureExt, TryFutureExt};
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::{body::Incoming, Request, Response};
use hyper_rustls::HttpsConnector;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Builder, Client},
    rt::TokioExecutor,
};
use tower_service::Service;



pub struct SgHttpClientConfig {
    pub tls_config: rustls::ClientConfig,
}

#[derive(Debug, Clone)]
pub struct SgHttpClient {
    inner: Client<HttpsConnector<HttpConnector>, BoxBody<Bytes, hyper::Error>>,
}

impl SgHttpClient {
    pub fn new<C: Into<Arc<rustls::ClientConfig>>>(tls_config: C) -> Self {
        let http_connector = HttpConnector::new();
        SgHttpClient {
            inner: Client::builder(TokioExecutor::new()).build(HttpsConnector::from((http_connector, tls_config))),
        }
    }
}

impl Service<Request<SgBody>> for SgHttpClient {
    type Response = Response<SgBody>;

    type Error = Infallible;

    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map(|_| Ok(()))
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        let (context, req) = req.into_context();
        let fut = self.inner.call(req).map_ok(|response| {
            let (parts, body) = response.into_parts();
            Response::<SgBody>::from_parts(parts, SgBody::with_context(body, context))
        });
        ResponseErrorFuture::new(DefaultErrorFormatter, fut).boxed()
    }
}


