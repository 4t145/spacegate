use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use crate::config::gateway_dto::{SgGateway, SgProtocol, SgTlsMode};
use core::task::{Context, Poll};
use http::{HeaderValue, Request, Response, StatusCode};

use super::http_route;
use lazy_static::lazy_static;
use serde_json::json;
use spacegate_tower::{
    listener::SgListen,
    service::{get_http_backend_service, http_backend_service},
    SgBoxService,
};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use std::{io, sync};
use tardis::tokio::time::timeout;
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log::{self},
    tokio::{self, sync::watch::Sender, task::JoinHandle},
    TardisFuns,
};
use tardis::{config::config_dto::LogConfig, consts::IP_UNSPECIFIED};
use tardis::{
    futures_util::{ready, FutureExt},
    tokio::sync::Mutex,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer, ServerConfig};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
    static ref START_JOIN_HANDLE: Arc<Mutex<HashMap<String, JoinHandle<()>>>> = <_>::default();
}

pub struct RunningSgGateway {
    token: CancellationToken,
    handle: JoinHandle<()>,
    shutdown_timeout: Duration,
}

impl RunningSgGateway {
    pub fn start(config: &SgGateway) -> TardisResult<Self> {
        let client = get_http_backend_service();
        if config.listeners.is_empty() {
            return Err(TardisError::bad_request("[SG.Server] Missing Listeners", ""));
        }
        if config.listeners.iter().any(|l| l.protocol != SgProtocol::Http && l.protocol != SgProtocol::Https && l.protocol != SgProtocol::Ws) {
            return Err(TardisError::bad_request("[SG.Server] Non-Http(s) protocols are not supported yet", ""));
        }
        if let Some(log_level) = config.parameters.log_level.clone() {
            log::debug!("[SG.Server] change log level to {log_level}");
            let fw_config = TardisFuns::fw_config();
            let old_configs = fw_config.log();
            let directive = format!("{domain}={log_level}", domain = crate::constants::DOMAIN_CODE).parse().expect("invalid directive");
            let mut directives = old_configs.directives.clone();
            if let Some(index) = directives.iter().position(|d| d.to_string().starts_with(crate::constants::DOMAIN_CODE)) {
                directives.remove(index);
            }
            directives.push(directive);
            TardisFuns::tracing().update_config(&LogConfig {
                level: old_configs.level.clone(),
                directives,
                ..Default::default()
            })?;
        }
        let cancel_token = CancellationToken::new();

        let gateway_name = Arc::new(config.name.to_string());
        let mut listens: Vec<SgListen<SgBoxService>> = Vec::new();
        for listener in &config.listeners {
            let ip = listener.ip.unwrap_or(IP_UNSPECIFIED);
            let addr = SocketAddr::new(ip, listener.port);

            let gateway_name = gateway_name.clone();
            let protocol = listener.protocol.to_string();
            let mut tls_cfg = None;
            if let Some(tls) = listener.tls.clone() {
                log::debug!("[SG.Server] Tls is init...mode:{:?}", tls.mode);
                if SgTlsMode::Terminate == tls.mode {
                    {
                        let certs = rustls_pemfile::certs(&mut tls.cert.as_bytes()).filter_map(Result::ok).collect::<Vec<_>>();
                        let keys = rustls_pemfile::read_all(&mut tls.key.as_bytes()).filter_map(Result::ok);

                        let key = keys
                            .find_map(|key| match key {
                                rustls_pemfile::Item::Pkcs1Key(k) => Some(PrivateKeyDer::Pkcs1(k)),
                                rustls_pemfile::Item::Pkcs8Key(k) => Some(PrivateKeyDer::Pkcs8(k)),
                                rustls_pemfile::Item::Sec1Key(k) => Some(PrivateKeyDer::Sec1(k)),
                                _ => None,
                            })
                            .ok_or(TardisError::internal_error("[SG.Server] Can not found a valid Tls private key", ""))?;

                        let mut tls_server_cfg = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;
                        tls_server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                        tls_cfg.replace(tls_server_cfg)
                    };
                }
            }
            let listen_id = format!("{name}/{protocol}", name = listener.name, protocol = protocol);
            let mut listen = SgListen::new(addr, client, cancel_token.cancelled_owned(), listen_id);
            if let Some(tls_cfg) = tls_cfg {
                listen.with_tls_config(tls_cfg)
            }
        }

        let task = tokio::spawn(async move {
            let join_set = tokio::task::JoinSet::new();
            for listen in listens {
                join_set.spawn(async move {
                    let id = listen.listener_id.clone();
                    if let Err(e) = listen.listen().await {
                        log::error!("[Sg.Server] listen error: {e}")
                    }
                    log::info!("[Sg.Server] listener[{id}] quit listening")
                })
            }
            while let Some(next) = join_set.join_next().await {}
        });
        Ok((cancel_token, task))
    }
    pub async fn shutdown(self) -> TardisResult<()>{
        self.token.cancel();
        match timeout(self.shutdown_timeout, self.handle).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                log::error!("[SG.Server] Join handle error:{e}");
            }
            Err(e) => {
                log::warn!("[SG.Server] Wait shutdown timeout:{e}");
                Ok(())
            }
        }?;
        log::info!("[SG.Server] Gateway shutdown");
        Ok(())
    }
}
