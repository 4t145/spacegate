// pub mod config;
pub mod body;
pub mod clients;
pub mod context;
pub mod helper_layers;
pub mod plugin_layers;
pub mod route_layers;
pub mod utils;

use std::{
    convert::Infallible,
    fmt::{self, Display},
    sync::Arc,
};

use body::dump::Dump;
use context::SgContext;
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

#[derive(Debug)]
pub struct SgBody {
    body: BoxBody<Bytes, hyper::Error>,
    dump: Option<Bytes>,
    context: SgContext,
}

impl Default for SgBody {
    fn default() -> Self {
        Self::empty()
    }
}

impl Body for SgBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        let mut pinned = std::pin::pin!(&mut self.body);
        pinned.as_mut().poll_frame(cx)
    }
}

impl SgBody {
    pub fn new(body: impl Body<Data = Bytes, Error = hyper::Error> + Send + Sync + 'static) -> Self {
        Self {
            body: BoxBody::new(body),
            context: SgContext::default(),
            dump: None,
        }
    }
    pub fn with_context(body: impl Body<Data = Bytes, Error = hyper::Error> + Send + Sync + 'static, context: SgContext) -> Self {
        Self {
            body: BoxBody::new(body),
            context,
            dump: None,
        }
    }
    pub fn empty() -> Self {
        Self {
            body: BoxBody::new(Empty::new().map_err(never)),
            context: SgContext::default(),
            dump: None,
        }
    }
    pub fn full(data: impl Into<Bytes>) -> Self {
        let bytes = data.into();
        Self {
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            context: SgContext::default(),
            dump: Some(bytes),
        }
    }
    pub fn into_context(self) -> (SgContext, BoxBody<Bytes, hyper::Error>) {
        (self.context, self.body)
    }
    pub async fn dump(self) -> Result<Self, hyper::Error> {
        let (context, body) = self.into_context();
        let bytes = body.collect().await?.to_bytes();
        Ok(Self {
            context,
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            dump: Some(bytes),
        })
    }
    pub fn dump_clone(&self) -> Option<Self> {
        self.dump.as_ref().map(|bytes| Self {
            context: self.context.clone(),
            body: BoxBody::new(Full::new(bytes.clone()).map_err(never)),
            dump: Some(bytes.clone()),
        })
    }
}

impl Clone for SgBody {
    fn clone(&self) -> Self {
        if let Some(dump) = self.dump_clone() {
            dump
        } else {
            panic!("SgBody can't be cloned before dump")
        }
    }
}

pub trait SgRequestExt {
    fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>);
}

impl SgRequestExt for Request<SgBody> {
    fn into_context(self) -> (SgContext, Request<BoxBody<Bytes, hyper::Error>>) {
        let (parts, body) = self.into_parts();
        let (context, body) = body.into_context();
        let real_body = Request::from_parts(parts, body);
        (context, real_body)
    }
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
    fn transpose(self) -> (SgContext, Response<BoxBody<Bytes, Error>>);
}

impl SgResponseExt for Response<SgBody> {
    fn with_code_message(code: StatusCode, message: impl Into<Bytes>) -> Self {
        let body = SgBody::full(message);
        Response::builder().status(code).body(body).expect("response builder error")
    }
    fn transpose(self) -> (SgContext, Response<BoxBody<Bytes, Error>>) {
        let (parts, body) = self.into_parts();
        let (context, body) = body.into_context();
        let real_body = Response::from_parts(parts, body);
        (context, real_body)
    }
}

pub type ReqOrResp = Result<Request<SgBody>, Response<SgBody>>;

type SgBoxService = BoxCloneService<Request<SgBody>, Response<SgBody>, Infallible>;
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
    use tower::ServiceExt;
    use tower_layer::Layer;
    use tower_service::Service;

    use crate::{
        clients::http_client::SgHttpClient,
        helper_layers::filter::{response_anyway::ResponseAnyway, FilterRequestLayer},
        plugin_layers::SgLayer,
        route_layers::http_route::{
            match_request::{MatchRequest, SgHttpPathMatch, SgHttpRouteMatch},
            SgHttpBackendLayer, SgHttpRouteLayer, SgHttpRouteRuleLayer,
        },
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
    async fn test() {
        let request = Request::builder().uri("http://example.com/hello").body(SgBody::full("hello spacegate")).unwrap();
        let r#match = SgHttpRouteMatch {
            path: Some(SgHttpPathMatch::Exact("/hello".to_string())),
            ..Default::default()
        };
        dbg!(r#match.match_request(&request));
        let http_router = SgHttpRouteLayer::builder()
            .hostnames(Some("example.com".to_string()))
            .rule(SgHttpRouteRuleLayer::builder().r#match(r#match).timeout(Duration::from_secs(5)).backend(SgHttpBackendLayer::builder()))
            .fallback(
                SgHttpRouteRuleLayer::builder().backend(SgHttpBackendLayer::builder().plugin(SgLayer(FilterRequestLayer::new(ResponseAnyway {
                    status: hyper::StatusCode::NOT_FOUND,
                    message: "[Sg.HttpRouteRule] no rule matched".to_string().into(),
                })))),
            )
            .build()
            .expect("");

        let mut test_service = http_router.layer(EchoService);
        let (ctx, response) =
            test_service.ready().await.unwrap().call(Request::builder().uri("http://example.com/hello").body(SgBody::full("hello spacegate")).unwrap()).await.unwrap().transpose();
        let (parts, body) = response.into_parts();
        dbg!(ctx, parts, body.collect().await.unwrap().to_bytes());
    }
}
