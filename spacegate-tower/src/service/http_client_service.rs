use std::{convert::Infallible, mem, pin::Pin, sync::Arc, time::Duration};
use tokio_rustls::rustls;
use crate::{
    helper_layers::response_error::{DefaultErrorFormatter, ResponseError, ResponseErrorFuture},
    plugin_layers::MakeSgLayer,
    SgBody, SgRequestExt, SgResponseExt, extension::reflect::Reflect,
};
use futures_util::{Future, FutureExt, TryFutureExt};
use http_body_util::combinators::BoxBody;
use hyper::{body::Incoming, Request, Response};
use hyper::{
    body::{Body, Bytes},
    StatusCode,
};
use hyper_rustls::HttpsConnector;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Builder, Client},
    rt::TokioExecutor,
};
use tower_service::Service;

pub fn get_client() -> SgHttpClient {
    todo!()
}

pub struct SgHttpClientConfig {
    pub tls_config: rustls::ClientConfig,
}

#[derive(Debug, Clone)]
pub struct SgHttpClient {
    inner: Client<HttpsConnector<HttpConnector>, SgBody>,
}

impl SgHttpClient {
    pub fn new<C: Into<Arc<rustls::ClientConfig>>>(tls_config: C) -> Self {
        let http_connector = HttpConnector::new();
        SgHttpClient {
            inner: Client::builder(TokioExecutor::new()).build(HttpsConnector::from((http_connector, tls_config))),
        }
    }
    pub async fn request(&mut self, mut req: Request<SgBody>) -> Response<SgBody> {
        let reflect = req.extensions_mut().remove::<Reflect>();
        match self.inner.request(req).await.map_err(Response::internal_error) {
            Ok(mut response) => {
                if let Some(reflect) = reflect {
                    response.extensions_mut().insert(reflect);
                }
                response.map(SgBody::new)
            }
            Err(err) => err,
        }
    }
    pub async fn request_timeout(&mut self, req: Request<SgBody>, timeout: Duration) -> Response<SgBody> {
        let fut = self.request(req);
        let resp = tokio::time::timeout(timeout, fut).await;
        match resp {
            Ok(resp) => resp,
            Err(_) => Response::with_code_message(StatusCode::GATEWAY_TIMEOUT, "request timeout"),
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
        let mut this = self.clone();
        mem::swap(&mut this, self);
        let fut = async move { this.request(req).map(Ok).await };
        fut.boxed()
    }
}
