//! This layer is used to make response's encoding compatible with the request's accept encoding.
//!
//! see also:
//! https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Accept-Encoding
//! https://developer.mozilla.org/zh-CN/docs/Web/HTTP/Headers/Content-Encoding
//!
//!

use std::convert::Infallible;
use std::{cmp::Ordering, str::FromStr};

// use crate::{plugin_layers::comde::content_encoding::ContentEncodingType, };
use futures_util::FutureExt;
use http_body_util::BodyExt;
use hyper::header::{HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING};
use hyper::{Request, Response};
use serde::{Deserialize, Serialize};
use spacegate_tower::{SgBody, SgBoxService};
use tower::{service_fn, BoxError, ServiceExt};
use tower_http::compression::{Compression as TowerCompression, CompressionLayer as TowerCompressionLayer};
use tower_http::decompression::{Decompression as TowerDecompression, DecompressionLayer as TowerDecompressionLayer};
use tower_layer::Layer;
use tower_service::Service;

pub struct ComdeLayer {}

pub struct DecompressionLayer;

impl DecompressionLayer {}

// impl Layer<SgBoxService> for DecompressionLayer {
//     type Service = SgBoxService;

//     fn layer(&self, inner: SgBoxService) -> Self::Service {
//         TowerCompression::new(inner)
//         inner
//     }
// }

// pub struct CompressionService<> {
//     inner: TowerCompression,
// }

// pub struct ComdeService {
//     inner: SgBoxService,
// }

// impl ComdeService {}

// pub fn echo_body<B: hyper::body::Body>(mut req: Request<B>) -> Response<B> {
//     Response::new(req.into_body())
// }

// impl ComdeService {
//     pub fn get_accept_encoding(&self, req: &Request<SgBody>) -> Option<CompressionType> {
//         req.request.headers().get(ACCEPT_ENCODING).and_then(|h| CompressionType::try_from(h).ok())
//     }
//     pub fn on_response(&self, mut resp: Response<SgBody>, accept: AcceptEncoding) -> Response<SgBody> {
//         let service = TowerDecompression::new(service_fn(echo_body));
//         let content_encoding = if let Some(s) = resp.response.headers().get(CONTENT_ENCODING) {
//             let Ok(content_encoding) = ContentEncodingType::try_from(s) else {
//                 return resp;
//             };
//             if accept.is_compatible(content_encoding) {
//                 return resp;
//             }
//             Some(content_encoding)
//         } else {
//             if accept.accept_identity() {
//                 return resp;
//             }
//             None
//         };
//         let target_type = accept.get_preferred().unwrap_or(accept_encoding::AcceptEncodingType::Identity);
//         match content_encoding {
//             Some(ContentEncodingType::Br) => {
//                 service.br(true);
//                 resp.response
//             }
//             Some(ContentEncodingType::Deflate) => {
//                 service.deflate(true);

//             }
//             Some(ContentEncodingType::Gzip) => {
//                 service.gzip(true);

//             }
//             ,
//             None => {
//             },
//         }
//         service.call();
//         match target_type {
//             accept_encoding::AcceptEncodingType::Br => {
//                 resp.map_body()
//             }
//             accept_encoding::AcceptEncodingType::Deflate => {
//                 resp.map_body()
//             }
//             accept_encoding::AcceptEncodingType::Gzip => {
//                 resp.map_body()
//             }
//             _ => {

//             }
//         }

//     }
// }

pub struct ComdecomService<S> {
    inner: TowerCompression<TowerDecompression<S>>,
}
impl<S> Service<Request<SgBody>> for ComdecomService<S>
where
    S: Service<Request<SgBody>, Response = Response<SgBody>, Error = Infallible>,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = <SgBoxService as Service<Request<SgBody>>>::Future;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        todo!()
    }

    fn call(&mut self, req: Request<SgBody>) -> Self::Future {
        let fut = self.inner.call(req);
        Box::pin(async move {
            let response = fut.await.expect("infallible");
            return Ok(response.map(|b| SgBody::new(b.map_err(Box::new))));
        })
    }
}
