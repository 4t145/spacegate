use std::sync::Arc;

use hyper::{Request, Response};
pub mod server;
pub mod sliding_window;
pub mod status_plugin;
use serde::{Deserialize, Serialize};
use spacegate_tower::{
    extension::BackendHost,
    helper_layers::{
        self,
        status::{self, Status},
    },
    layers::gateway::builder::SgGatewayLayerBuilder,
    SgBody, SgBoxLayer,
};
use tardis::{
    chrono::{Duration, Utc},
    tokio::{self, sync::RwLock},
};
use tower::BoxError;

use crate::MakeSgLayer;

use self::{
    sliding_window::SlidingWindowCounter,
    status_plugin::{get_status, update_status},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterStatusConfig {
    #[serde(alias = "serv_addr")]
    pub host: String,
    pub port: u16,
    pub title: String,
    /// Unhealthy threshold , if server error more than this, server will be tag as unhealthy
    pub unhealthy_threshold: u16,
    /// second
    pub interval: u64,
    pub status_cache_key: String,
    pub window_cache_key: String,
}

impl Default for SgFilterStatusConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8110,
            title: "System Status".to_string(),
            unhealthy_threshold: 3,
            interval: 5,
            status_cache_key: "spacegate:cache:plugin:status".to_string(),
            window_cache_key: sliding_window::DEFAULT_CONF_WINDOW_KEY.to_string(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct NoCachePolicy {
    counter: Arc<RwLock<SlidingWindowCounter>>,
    unhealthy_threshold: u16,
}

impl status::Policy for NoCachePolicy {
    fn on_request(&self, _req: &Request<SgBody>) {
        // do nothing
    }

    fn on_response(&self, resp: &Response<SgBody>) {
        if let Some(backend_host) = resp.extensions().get::<BackendHost>() {
            let backend_host = backend_host.0.clone();
            let unhealthy_threshold = self.unhealthy_threshold;
            let counter = self.counter.clone();
            if resp.status().is_server_error() {
                let now = Utc::now();
                tardis::tokio::spawn(async move {
                    let mut counter = counter.write().await;
                    let count = counter.add_and_count(now);
                    if count >= unhealthy_threshold as u64 {
                        update_status(&backend_host, status_plugin::Status::Major).await?;
                    } else {
                        update_status(&backend_host, status_plugin::Status::Minor).await?;
                    }
                    Result::<_, BoxError>::Ok(())
                });
            } else {
                tardis::tokio::spawn(async move {
                    if let Some(status) = get_status(&backend_host).await? {
                        if status != status_plugin::Status::Good {
                            update_status(&backend_host, status_plugin::Status::Good).await?;
                        }
                    }
                    Result::<_, BoxError>::Ok(())
                });
            }
        }
    }
}

impl MakeSgLayer for SgFilterStatusConfig {
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError> {
        Err(BoxError::from("status plugin is only supported on gateway layer"))
    }
    fn install_on_gateway(&self, gateway: SgGatewayLayerBuilder) -> Result<SgGatewayLayerBuilder, BoxError> {
        let gateway_name = gateway.gateway_name.clone();
        let cancel_guard = gateway.cancel_token.clone();
        let config = self.clone();
        tokio::spawn(async move {
            if let Err(e) = server::launch_status_server(&config, gateway_name, cancel_guard).await {
                tracing::error!("[SG.Filter.Status] launch status server error: {e}");
            }
        });

        #[cfg(feature = "cache")]
        unimplemented!("cache feature is not supported yet");
        #[cfg(not(feature = "cache"))]
        let layer = {
            let counter = Arc::new(RwLock::new(SlidingWindowCounter::new(Duration::seconds(self.interval as i64), 60)));
            let policy = NoCachePolicy {
                counter,
                unhealthy_threshold: self.unhealthy_threshold,
            };
            SgBoxLayer::new(helper_layers::status::StatusLayer::new(policy))
        };
        Ok(gateway.http_plugin(layer))
    }
}

// #[async_trait]
// impl SgPluginFilter for SgFilterStatus {
//     fn accept(&self) -> super::SgPluginFilterAccept {
//         super::SgPluginFilterAccept {
//             kind: vec![super::SgPluginFilterKind::Http],
//             accept_error_response: true,
//         }
//     }

//     async fn init(&mut self, init_dto: &SgPluginFilterInitDto) -> TardisResult<()> {
//         if !init_dto.attached_level.eq(&SgAttachedLevel::Gateway) {
//             log::error!("[SG.Filter.Status] init filter is only can attached to gateway");
//             return Ok(());
//         }
//         let (shutdown_tx, _) = tokio::sync::watch::channel(());
//         let mut shutdown_rx = shutdown_tx.subscribe();

//         let mut shutdown = SHUTDOWN_TX.lock().await;
//         if let Some(old_shutdown) = shutdown.remove(&self.port) {
//             old_shutdown.0.send(()).ok();
//             let _ = old_shutdown.1.await;
//             log::trace!("[SG.Filter.Status] init stop old service.");
//         }

//         let addr_ip: IpAddr = self.serv_addr.parse().map_err(|e| TardisError::conflict(&format!("[SG.Filter.Status] serv_addr parse error: {e}"), ""))?;
//         let addr = (addr_ip, self.port).into();
//         let title = Arc::new(Mutex::new(self.title.clone()));
//         let gateway_name = Arc::new(Mutex::new(init_dto.gateway_name.clone()));
//         let cache_key = Arc::new(Mutex::new(get_cache_key(self, &init_dto.gateway_name)));
//         let make_svc = make_service_fn(move |_conn| {
//             let title = title.clone();
//             let gateway_name = gateway_name.clone();
//             let cache_key = cache_key.clone();
//             async move {
//                 Ok::<_, hyper::Error>(service_fn(move |request: Request<Body>| {
//                     status_plugin::create_status_html(request, gateway_name.clone(), cache_key.clone(), title.clone())
//                 }))
//             }
//         });

//         let server = match Server::try_bind(&addr) {
//             Ok(server) => server.serve(make_svc),
//             Err(e) => return Err(TardisError::conflict(&format!("[SG.Filter.Status] bind error: {e}"), "")),
//         };

//         let join = tokio::spawn(async move {
//             log::info!("[SG.Filter.Status] Server started: {addr}");
//             let server = server.with_graceful_shutdown(async move {
//                 shutdown_rx.changed().await.ok();
//             });
//             server.await
//         });
//         (*shutdown).insert(self.port, (shutdown_tx, join));

//         #[cfg(feature = "cache")]
//         {
//             clean_status(&get_cache_key(self, &init_dto.gateway_name), &init_dto.gateway_name).await?;
//         }
//         #[cfg(not(feature = "cache"))]
//         {
//             clean_status().await?;
//         }
//         for http_route_rule in init_dto.http_route_rules.clone() {
//             if let Some(backends) = &http_route_rule.backends {
//                 for backend in backends {
//                     #[cfg(feature = "cache")]
//                     {
//                         let cache_client = cache_client::get(&init_dto.gateway_name).await?;
//                         update_status(
//                             &backend.name_or_host,
//                             &get_cache_key(self, &init_dto.gateway_name),
//                             &cache_client,
//                             status_plugin::Status::default(),
//                         )
//                         .await?;
//                     }
//                     #[cfg(not(feature = "cache"))]
//                     {
//                         update_status(&backend.name_or_host, status_plugin::Status::default()).await?;
//                     }
//                 }
//             }
//         }
//         #[cfg(not(feature = "cache"))]
//         {
//             self.counter = RwLock::new(SlidingWindowCounter::new(Duration::seconds(self.interval as i64), 60));
//         }
//         Ok(())
//     }

//     async fn destroy(&self) -> TardisResult<()> {
//         let mut shutdown = SHUTDOWN_TX.lock().await;

//         if let Some(shutdown) = shutdown.remove(&self.port) {
//             shutdown.0.send(()).ok();
//             let _ = shutdown.1.await;
//             log::info!("[SG.Filter.Status] Server stopped");
//         };
//         Ok(())
//     }

//     async fn req_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
//         Ok((true, ctx))
//     }

//     async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
//         if let Some(backend_name) = ctx.get_chose_backend_name() {
//             if ctx.is_resp_error() {
//                 let now = Utc::now();
//                 let count;
//                 #[cfg(not(feature = "cache"))]
//                 {
//                     let mut counter = self.counter.write().await;
//                     count = counter.add_and_count(now)
//                 }
//                 #[cfg(feature = "cache")]
//                 {
//                     count = SlidingWindowCounter::new(Duration::seconds(self.interval as i64), &self.window_cache_key).add_and_count(now, &ctx).await?;
//                 }
//                 if count >= self.unhealthy_threshold as u64 {
//                     #[cfg(feature = "cache")]
//                     {
//                         update_status(
//                             &backend_name,
//                             &get_cache_key(self, &ctx.get_gateway_name()),
//                             &ctx.cache().await?,
//                             status_plugin::Status::Major,
//                         )
//                         .await?;
//                     }
//                     #[cfg(not(feature = "cache"))]
//                     {
//                         update_status(&backend_name, status_plugin::Status::Major).await?;
//                     }
//                 } else {
//                     #[cfg(feature = "cache")]
//                     {
//                         update_status(
//                             &backend_name,
//                             &get_cache_key(self, &ctx.get_gateway_name()),
//                             &ctx.cache().await?,
//                             status_plugin::Status::Minor,
//                         )
//                         .await?;
//                     }
//                     #[cfg(not(feature = "cache"))]
//                     {
//                         update_status(&backend_name, status_plugin::Status::Minor).await?;
//                     }
//                 }
//             } else {
//                 let gotten_status: Option<Status>;
//                 #[cfg(feature = "cache")]
//                 {
//                     gotten_status = get_status(&backend_name, &get_cache_key(self, &ctx.get_gateway_name()), &ctx.cache().await?).await?;
//                 }
//                 #[cfg(not(feature = "cache"))]
//                 {
//                     gotten_status = get_status(&backend_name).await?;
//                 }
//                 if let Some(status) = gotten_status {
//                     if status != status_plugin::Status::Good {
//                         #[cfg(feature = "cache")]
//                         {
//                             update_status(
//                                 &backend_name,
//                                 &get_cache_key(self, &ctx.get_gateway_name()),
//                                 &ctx.cache().await?,
//                                 status_plugin::Status::Good,
//                             )
//                             .await?;
//                         }
//                         #[cfg(not(feature = "cache"))]
//                         {
//                             update_status(&backend_name, status_plugin::Status::Good).await?;
//                         }
//                     }
//                 }
//             }
//         }
//         Ok((true, ctx))
//     }
// }
