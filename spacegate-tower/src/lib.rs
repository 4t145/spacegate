// pub mod config;
pub mod body;
pub mod extension;
pub mod helper_layers;
pub mod plugin_layers;
pub mod layers;
pub mod service;
pub mod utils;

pub use body::SgBody;
use extension::reflect::Reflect;
use std::{
    convert::Infallible,
    fmt::{self, Display},
    sync::Arc,
};

use body::dump::Dump;
use helper_layers::response_error::ErrorFormatter;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};

use hyper::{
    body::{Body, Bytes},
    Error, Request, Response, StatusCode,
};
use tardis::tokio;
use tower::util::BoxCloneService;
use tower_layer::{layer_fn, Layer};
use tower_service::Service;
use utils::{fold_sg_layers::fold_sg_layers, never};

pub trait SgRequestExt {
    fn with_reflect(&mut self);
    // fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>);
}

impl SgRequestExt for Request<SgBody> {
    fn with_reflect(&mut self) {
        self.extensions_mut().insert(Reflect::new());
    }
    // fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>) {
    //     let (parts, body) = self.into_parts();
    //     let (context, body) = body.into_context();
    //     let real_body = Request::from_parts(parts, body);
    //     (context, real_body)
    // }
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
        let mut resp = Response::builder().status(code).body(body).expect("response builder error");
        resp.extensions_mut().insert(Reflect::new());
        resp
    }
}

pub type ReqOrResp = Result<Request<SgBody>, Response<SgBody>>;

pub type SgBoxService = BoxCloneService<Request<SgBody>, Response<SgBody>, Infallible>;
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

#[cfg(test)]
mod test {
    use std::{
        convert::Infallible,
        future::{ready, Ready},
        time::Duration,
    };

    use http_body_util::BodyExt;
    use hyper::{Request, Response};
    use tardis::tokio;
    use tower::{BoxError, ServiceExt};
    use tower_layer::Layer;
    use tower_service::Service;

    use crate::{
        helper_layers::filter::{response_anyway::ResponseAnyway, FilterRequestLayer},
        plugin_layers::SgLayer,
        layers::http_route::{
            match_request::{MatchRequest, SgHttpPathMatch, SgHttpRouteMatch},
            SgHttpBackendLayer, SgHttpRouteLayer, SgHttpRouteRuleLayer,
        },
        service::http_client_service::SgHttpClient,
        SgBody, SgResponseExt,
    };
    #[derive(Clone)]
    pub struct EchoService;

    impl<B> tower_service::Service<Request<B>> for EchoService {
        type Response = Response<B>;

        type Error = Infallible;

        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request<B>) -> Self::Future {
            ready(Ok(Response::new(req.into_body())))
        }
    }

    #[tokio::test]
    async fn test() -> Result<(), BoxError> {
        let request = Request::builder().uri("http://example.com/hello").body(SgBody::full("hello spacegate")).unwrap();
        let r#match = SgHttpRouteMatch {
            path: Some(SgHttpPathMatch::Exact("/hello".to_string())),
            ..Default::default()
        };
        dbg!(r#match.match_request(&request));
        let http_router = SgHttpRouteLayer::builder()
            .hostnames(Some("example.com".to_string()))
            .rule(SgHttpRouteRuleLayer::builder().r#match(r#match).timeout(Duration::from_secs(5)).backend(SgHttpBackendLayer::builder().build()?).build()?)
            .fallback(
                SgHttpRouteRuleLayer::builder()
                    .backend(
                        SgHttpBackendLayer::builder()
                            .plugin(SgLayer(FilterRequestLayer::new(ResponseAnyway {
                                status: hyper::StatusCode::NOT_FOUND,
                                message: "[Sg.HttpRouteRule] no rule matched".to_string().into(),
                            })))
                            .build()?,
                    )
                    .build()?,
            )
            .build()?;
        let mut test_service = http_router.layer(EchoService);
        let (parts, body) =
            test_service.ready().await.unwrap().call(Request::builder().uri("http://example.com/hello").body(SgBody::full("hello spacegate")).unwrap()).await.unwrap().into_parts();
        dbg!(parts, body.collect().await.unwrap().to_bytes());
        Ok(())
    }
}