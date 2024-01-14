use std::io::Bytes;

use async_compression::tokio::bufread::GzipEncoder;
use http_body_util::{
    combinators::{BoxBody, UnsyncBoxBody},
    BodyExt, BodyStream, StreamBody,
};
use hyper::body::Body;
use futures_util::{StreamExt, TryStreamExt};
use tokio_util::io::StreamReader;
use tower_http::compression::CompressionBody;
pub struct GzipBody<D, E> {
    inner: BoxBody<D, E>,
}

impl<D, E> GzipBody<D, E>
where
    BoxBody<D, E>: Body,
{
    pub fn new<B: Body>(inner: B) -> Self {
        todo!()
        // let stream = BodyStream::new(inner);
        // let gzip_stream = stream.map_ok(
        //     |data| 
        //     if 
        // );
        // let gzip_body = StreamBody::new(gzip_stream);
        // GzipEncoder::new(StreamReader::new(stream));
        // Self { inner }
    }
}


