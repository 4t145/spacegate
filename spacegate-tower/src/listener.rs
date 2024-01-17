use futures_util::{io, Future};
use hyper::{body::Incoming, rt::Executor, Request, Response};
use hyper_util::rt::{self, TokioExecutor, TokioIo};
use rustls::pki_types::PrivateKeyDer;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use std::{
    convert::Infallible,
    fmt::Display,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};
use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tower::BoxError;
use tracing::instrument;

use crate::SgBody;

/// Listener embodies the concept of a logical endpoint where a Gateway accepts network connections.
#[derive(Clone)]
pub struct SgListen<S> {
    pub socket_addr: SocketAddr,
    pub service: S,
    pub tls_cfg: Option<Arc<rustls::ServerConfig>>,
    pub cancel_token: CancellationToken,
    pub listener_id: String,
}

impl<S> std::fmt::Debug for SgListen<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgListen").field("socket_addr", &self.socket_addr).field("tls_enabled", &self.tls_cfg.is_some()).field("listener_id", &self.listener_id).finish()
    }
}

impl<S> SgListen<S> {
    pub fn new(socket_addr: SocketAddr, service: S, cancel_token: CancellationToken, id: impl Into<String>) -> Self {
        Self {
            socket_addr,
            service,
            tls_cfg: None,
            cancel_token,
            listener_id: id.into(),
        }
    }
    pub fn with_tls_config(mut self, tls_cfg: impl Into<Arc<rustls::ServerConfig>>) -> Self {
        self.tls_cfg = Some(tls_cfg.into());
        self
    }
}
// fn temp() {
//     if let Some(tls) = &self.tls {
//         tracing::debug!("[SG.Server] Tls is init...mode:{:?}", tls.mode);
//         if SgTlsMode::Terminate == tls.mode {
//             {
//                 let certs = rustls_pemfile::certs(&mut tls.cert.as_bytes()).filter_map(Result::ok).collect::<Vec<_>>();
//                 let keys = rustls_pemfile::read_all(&mut tls.key.as_bytes()).filter_map(Result::ok);

//                 let key = keys.find_map(|key| {
//                     match key {
//                         rustls_pemfile::Item::Pkcs1Key(k) => Some(PrivateKeyDer::Pkcs1(k)),
//                         rustls_pemfile::Item::Pkcs8Key(k) => Some(PrivateKeyDer::Pkcs8(k)),
//                         rustls_pemfile::Item::Sec1Key(k) => Some(PrivateKeyDer::Sec1(k)),
//                         _ => None,
//                     }
//                 }).ok_or(BoxError::from("[SG.Server] Can not found a valid Tls private key"))?;

//                 let mut cfg = rustls::ServerConfig::builder()

//                     .with_no_client_auth()
//                     .with_single_cert(certs, key)
//                     ?;
//                 cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
//                 Arc::new(cfg)
//             };
//         }
//     };
// }

#[derive(Debug, Clone)]
pub struct HyperServiceAdapter<S> {
    service: S,
    peer: SocketAddr,
}
impl<S> HyperServiceAdapter<S> {
    pub fn new(service: S, peer: SocketAddr) -> Self {
        Self { service, peer }
    }
}

impl<S> hyper::service::Service<Request<Incoming>> for HyperServiceAdapter<S>
where
    S: tower::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send  + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<SgBody>;
    type Error = Infallible;
    type Future = S::Future;

    #[inline]
    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        req.extensions_mut().insert(self.peer);
        self.service.clone().call(req.map(SgBody::new))
    }
}

impl<S> SgListen<S>
where
    S: tower::Service<Request<SgBody>, Error = Infallible, Response = Response<SgBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    #[instrument(skip(stream, service, tls_cfg))]
    pub async fn accept(stream: TcpStream, peer_addr: SocketAddr, tls_cfg: Option<Arc<rustls::ServerConfig>>, service: S) -> Result<(), BoxError> {
        tracing::debug!("[Sg.Listen] Accepted connection");
        let builder = hyper_util::server::conn::auto::Builder::new(rt::TokioExecutor::default());
        let service = HyperServiceAdapter::new(service, peer_addr);
        match tls_cfg {
            Some(tls_cfg) => {
                let connector = tokio_rustls::TlsAcceptor::from(tls_cfg);
                let accepted = connector.accept(stream).await?;
                let io = TokioIo::new(accepted);
                let conn = builder.serve_connection_with_upgrades(io, service);
                conn.await?;
            }
            None => {
                let io = TokioIo::new(stream);
                let conn = builder.serve_connection_with_upgrades(io, service);
                conn.await?;
            }
        }
        tracing::debug!("[Sg.Listen] Connection closed");
        Ok(())
    }
    #[instrument()]
    pub async fn listen(self) -> Result<(), BoxError> {
        let listener = tokio::net::TcpListener::bind(self.socket_addr).await?;
        let cancel_token = self.cancel_token;
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    tracing::warn!("[Sg.Listen] cancelled");
                    return Ok(());
                },
                accepted = listener.accept() => {
                    match accepted {
                        Ok((stream, peer_addr)) => {
                            let service = self.service.clone();
                            let tls_cfg = self.tls_cfg.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::accept(stream, peer_addr, tls_cfg, service).await {
                                    tracing::warn!("[Sg.Listen] Accept stream error: {:?}", e);
                                }
                            });
                        },
                        Err(e) => {
                            tracing::warn!("[Sg.Listen] Accept tcp connection error: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}
