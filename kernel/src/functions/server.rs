use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Mutex,
};

use crate::config::{
    gateway_dto::{SgGateway, SgProtocol, SgTlsMode},
    http_route_dto::SgHttpRoute,
    plugin_filter_dto::SgRouteFilter,
};
use core::task::{Context, Poll};
use http::{HeaderValue, Request, Response, StatusCode};

use lazy_static::lazy_static;
use serde_json::json;
use spacegate_tower::{
    layers::gateway::SgGatewayLayer,
    listener::SgListen,
    service::{get_http_backend_service, http_backend_service},
    BoxError, Layer, Service, SgBoxService,
};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::vec::Vec;
use std::{io, sync};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::future::join_all,
    log::{self as tracing, debug, info},
    log::{self, error},
    tokio::{self, sync::watch::Sender, task::JoinHandle},
    TardisFuns,
};
use tardis::{config::config_dto::LogConfig, consts::IP_UNSPECIFIED};
use tardis::{
    futures_util::{ready, FutureExt},
    log::{instrument, warn},
};
use tardis::{tardis_static, tokio::time::timeout};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer, ServerConfig};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

lazy_static! {
    static ref SHUTDOWN_TX: Arc<Mutex<HashMap<String, Sender<()>>>> = <_>::default();
    static ref START_JOIN_HANDLE: Arc<Mutex<HashMap<String, JoinHandle<()>>>> = <_>::default();
}

/// Create a gateway service from plugins and http_routes
fn create_service(plugins: Vec<SgRouteFilter>, http_routes: Vec<crate::SgHttpRoute>) -> Result<SgBoxService, BoxError> {
    let routes = http_routes
        .into_iter()
        .map(|route| {
            let plugins = route.filters.unwrap_or_default();
            let plugins = plugins.into_iter().map(SgRouteFilter::into_layer).collect::<Result<Vec<_>, _>>()?;
            let rules = route.rules.unwrap_or_default();
            let rules = rules
                .into_iter()
                .map(|route_rule| {
                    let mut builder = spacegate_tower::layers::http_route::SgHttpRouteRuleLayer::builder();
                    builder = if let Some(matches) = route_rule.matches {
                        builder.matches(matches)
                    } else {
                        builder.match_all()
                    };
                    if let Some(backends) = route_rule.backends {
                        let backends = backends
                            .into_iter()
                            .map(|backend| {
                                let host = backend.get_host();
                                let mut builder = spacegate_tower::layers::http_route::SgHttpBackendLayer::builder();
                                let plugins = backend.filters.unwrap_or_default();
                                let plugins = plugins.into_iter().map(SgRouteFilter::into_layer).collect::<Result<Vec<_>, _>>()?;
                                builder = builder.host(host).port(backend.port).plugins(plugins);
                                let protocol = backend.protocol;
                                if let Some(protocol) = protocol {
                                    builder = builder.protocol(protocol.to_string());
                                }
                                builder.build()
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        builder = builder.backends(backends);
                    };
                    if let Some(timeout) = route_rule.timeout_ms {
                        builder = builder.timeout(Duration::from_millis(timeout));
                    }
                    let plugins = route_rule.filters.unwrap_or_default();
                    builder = builder.plugins(plugins.into_iter().map(SgRouteFilter::into_layer).collect::<Result<Vec<_>, _>>()?);
                    builder.build()
                })
                .collect::<Result<Vec<_>, _>>()?;
            spacegate_tower::layers::http_route::SgHttpRoute::builder().hostnames(route.hostnames.unwrap_or_default()).plugins(plugins).rules(rules).build()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let plugins = plugins.into_iter().map(SgRouteFilter::into_layer).collect::<Result<Vec<_>, _>>()?;
    let gateway_layer = spacegate_tower::layers::gateway::SgGatewayLayer::builder().http_routers(routes).http_plugins(plugins).build();
    let backend_service = get_http_backend_service();
    let service = SgBoxService::new(gateway_layer.layer(backend_service));
    Ok(service)
}

/// # Gateway
/// A running spacegate gateway instance
///
/// It's created by calling [start](RunningSgGateway::start).
///
/// And you can use [shutdown](RunningSgGateway::shutdown) to shutdown it manually.
pub struct RunningSgGateway {
    token: CancellationToken,
    handle: JoinHandle<()>,
    shutdown_timeout: Duration,
}

impl RunningSgGateway {
    tardis_static! {
        pub global_store: Arc<Mutex<HashMap<String, RunningSgGateway>>>;
    }

    pub fn global_save(gateway_name: impl Into<String>, gateway: RunningSgGateway) {
        let mut global_store = Self::global_store().lock().expect("poisoned lock");
        global_store.insert(gateway_name.into(), gateway);
    }

    pub fn global_remove(gateway_name: impl AsRef<str>) -> Option<RunningSgGateway> {
        let mut global_store = Self::global_store().lock().expect("poisoned lock");
        global_store.remove(gateway_name.as_ref())
    }

    /// Start a gateway from plugins and http_routes
    #[instrument(fields(gateway=%config.name), skip_all, err)]
    pub fn start(config: SgGateway, http_routes: Vec<SgHttpRoute>) -> Result<Self, BoxError> {
        let service = create_service(config.filters.unwrap_or_default(), http_routes)?;
        if config.listeners.is_empty() {
            return Err("[SG.Server] Missing Listeners".into());
        }
        if config.listeners.iter().any(|l| l.protocol != SgProtocol::Http && l.protocol != SgProtocol::Https && l.protocol != SgProtocol::Ws) {
            return Err("[SG.Server] Non-Http(s) protocols are not supported yet".into());
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
                        let mut tls_key = tls.key.as_bytes();
                        let mut keys = rustls_pemfile::read_all(&mut tls_key).filter_map(Result::ok);

                        let key = keys.find_map(|key| {
                            debug!("key item: {:?}", key);
                            match key {
                                rustls_pemfile::Item::Pkcs1Key(k) => Some(PrivateKeyDer::Pkcs1(k)),
                                rustls_pemfile::Item::Pkcs8Key(k) => Some(PrivateKeyDer::Pkcs8(k)),
                                rustls_pemfile::Item::Sec1Key(k) => Some(PrivateKeyDer::Sec1(k)),
                                rest => {
                                    warn!("Unsupported key type: {:?}", rest);
                                    None
                                }
                            }
                        });
                        if let Some(key) = key {
                            info!("[SG.Server] using cert key {key:?}");
                            let mut tls_server_cfg = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;
                            tls_server_cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec(), b"http/1.0".to_vec()];
                            tls_cfg.replace(tls_server_cfg);
                        } else {
                            error!("[SG.Server] Can not found a valid Tls private key");
                        }
                    };
                }
            }
            let listen_id = format!("{gateway_name}-{name}-{protocol}", name = listener.name.as_deref().unwrap_or("?"), protocol = protocol);
            let mut listen = SgListen::new(addr, service.clone(), cancel_token.clone(), listen_id);
            if let Some(tls_cfg) = tls_cfg {
                listen = listen.with_tls_config(tls_cfg);
            }
            listens.push(listen)
        }

        let task = tokio::spawn(async move {
            let mut join_set = tokio::task::JoinSet::new();
            for listen in listens {
                join_set.spawn(async move {
                    let id = listen.listener_id.clone();
                    if let Err(e) = listen.listen().await {
                        log::error!("[Sg.Server] listen error: {e}")
                    }
                    log::info!("[Sg.Server] listener[{id}] quit listening")
                });
            }
            while (join_set.join_next().await).is_some() {}
        });
        Ok(RunningSgGateway {
            token: cancel_token,
            handle: task,
            shutdown_timeout: Duration::from_secs(10),
        })
    }

    /// Shutdown this gateway
    pub async fn shutdown(self) {
        self.token.cancel();
        match timeout(self.shutdown_timeout, self.handle).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                log::error!("[SG.Server] Join handle error:{e}");
            }
            Err(e) => {
                log::warn!("[SG.Server] Wait shutdown timeout:{e}");
            }
        };
        log::info!("[SG.Server] Gateway shutdown");
    }
}
