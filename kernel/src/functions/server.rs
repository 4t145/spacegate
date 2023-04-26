use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
};

use crate::config::gateway_dto::{SgGateway, SgProtocol};
use core::task::{Context, Poll};
use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use lazy_static::lazy_static;
use rustls::{PrivateKey, ServerConfig};
use std::pin::Pin;
use std::sync::Arc;
use std::vec::Vec;
use std::{io, sync};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log,
    tokio::{self, sync::watch::Sender},
};
use tardis::{
    futures_util::{ready, FutureExt},
    tokio::sync::Mutex,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::http_route;

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
}

pub async fn init(gateway_conf: &SgGateway) -> TardisResult<Vec<SgServerInst>> {
    if gateway_conf.listeners.is_empty() {
        return Err(TardisError::bad_request("[SG.server] Missing Listeners", ""));
    }
    if gateway_conf.listeners.iter().any(|l| l.protocol != SgProtocol::Http && l.protocol != SgProtocol::Https) {
        return Err(TardisError::bad_request("[SG.server] Non-Http(s) protocols are not supported yet", ""));
    }
    let (shutdown_tx, _) = tokio::sync::watch::channel(());

    let gateway_name = Arc::new(gateway_conf.name.to_string());
    let mut server_insts: Vec<SgServerInst> = Vec::new();
    for listener in &gateway_conf.listeners {
        let ip = listener.ip.as_deref().unwrap_or("0.0.0.0");
        let addr = if ip.contains('.') {
            let ip: Ipv4Addr = ip.parse().map_err(|_| TardisError::bad_request(&format!("[SG.server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V4(ip), listener.port)
        } else {
            let ip: Ipv6Addr = ip.parse().map_err(|_| TardisError::bad_request(&format!("[SG.server] IP {ip} is not legal"), ""))?;
            SocketAddr::new(std::net::IpAddr::V6(ip), listener.port)
        };

        let mut shutdown_rx = shutdown_tx.subscribe();

        let gateway_name = gateway_name.clone();
        if let Some(tls) = &listener.tls {
            let tls_cfg = {
                let certs =
                    rustls_pemfile::certs(&mut tls.cert.as_bytes()).map_err(|error| TardisError::bad_request(&format!("[SG.server] Tls certificates not legal: {error}"), ""))?;
                let certs = certs.into_iter().map(rustls::Certificate).collect::<Vec<_>>();
                let key = rustls_pemfile::rsa_private_keys(&mut tls.key.as_bytes())
                    .map_err(|error| TardisError::bad_request(&format!("[SG.server] Tls private keys not legal: {error}"), ""))?;
                let key = PrivateKey(key[0].clone());
                let mut cfg = rustls::ServerConfig::builder()
                    .with_safe_defaults()
                    .with_no_client_auth()
                    .with_single_cert(certs, key)
                    .map_err(|error| TardisError::bad_request(&format!("[SG.server] Tls not legal: {error}"), ""))?;
                cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                sync::Arc::new(cfg)
            };
            let incoming = AddrIncoming::bind(&addr).map_err(|error| TardisError::bad_request(&format!("[SG.server] Bind address error: {error}"), ""))?;
            let server = Server::builder(TlsAcceptor::new(tls_cfg, incoming)).serve(make_service_fn(move |client: &TlsStream| {
                let remote_addr = match &client.state {
                    State::Handshaking(addr) => addr.get_ref().unwrap().remote_addr(),
                    State::Streaming(addr) => addr.get_ref().0.remote_addr(),
                };
                let gateway_name = gateway_name.clone();
                async move { Ok::<_, Infallible>(service_fn(move |req| http_route::process(gateway_name.clone(), "https", remote_addr, req))) }
            }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            server_insts.push(SgServerInst { addr, server: server.boxed() });
        } else {
            let server = Server::bind(&addr).serve(make_service_fn(move |client: &AddrStream| {
                let remote_addr = client.remote_addr();
                let gateway_name = gateway_name.clone();
                async move { Ok::<_, Infallible>(service_fn(move |req| http_route::process(gateway_name.clone(), "http", remote_addr, req))) }
            }));
            let server = server.with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
            });
            server_insts.push(SgServerInst { addr, server: server.boxed() });
        }
    }

    let mut shutdown = SHUTDOWN_TX.lock().await;
    shutdown.insert(gateway_name.to_string(), shutdown_tx);

    Ok(server_insts)
}

pub async fn startup(servers: Vec<SgServerInst>) -> TardisResult<()> {
    for server in &servers {
        log::info!("[SG.server] Listening on http://{} ", server.addr);
    }
    let servers = servers.into_iter().map(|s| s.server).collect::<Vec<_>>();
    tokio::spawn(async move {
        join_all(servers).await;
    });
    Ok(())
}

pub async fn shutdown(gateway_name: &str) -> TardisResult<()> {
    let mut shutdown = SHUTDOWN_TX.lock().await;
    if let Some(shutdown_tx) = shutdown.remove(gateway_name) {
        shutdown_tx.send(()).map_err(|_| TardisError::bad_request("[SG.server] Shutdown failed", ""))?;
    }
    Ok(())
}

struct TlsAcceptor {
    config: Arc<ServerConfig>,
    incoming: AddrIncoming,
}

impl TlsAcceptor {
    pub fn new(config: Arc<ServerConfig>, incoming: AddrIncoming) -> TlsAcceptor {
        TlsAcceptor { config, incoming }
    }
}

impl Accept for TlsAcceptor {
    type Conn = TlsStream;
    type Error = io::Error;

    fn poll_accept(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let pin = self.get_mut();
        match ready!(Pin::new(&mut pin.incoming).poll_accept(cx)) {
            Some(Ok(sock)) => Poll::Ready(Some(Ok(TlsStream::new(sock, pin.config.clone())))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}

enum State {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

struct TlsStream {
    state: State,
}

impl TlsStream {
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> TlsStream {
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        TlsStream {
            state: State::Handshaking(accept),
        }
    }
}

impl AsyncRead for TlsStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TlsStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub struct SgServerInst {
    pub addr: SocketAddr,
    pub server: Pin<Box<dyn std::future::Future<Output = Result<(), hyper::Error>> + std::marker::Send>>,
}
