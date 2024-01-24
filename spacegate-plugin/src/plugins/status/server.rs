use std::{net::IpAddr, str::FromStr, sync::Arc};

use super::{status_plugin, SgFilterStatusConfig};
use http_body_util::Full;
use hyper::{service::service_fn, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tardis::tokio::{self, sync::Mutex};
use tower::BoxError;

pub async fn launch_status_server(config: &SgFilterStatusConfig) -> Result<(), BoxError> {
    let host = IpAddr::from_str(&config.host)?;
    let port = config.port;
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    let connector = hyper_util::server::conn::auto::Builder::new(TokioExecutor::default());
    let gateway_name = Arc::new(Mutex::new(config.gateway_name.clone()));
    
    'accept_loop: loop {
        let (stream, peer) = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break 'accept_loop;
            }
            accept = listener.accept() => {
                match accept {
                    Ok(incoming) => incoming,
                    Err(e) => {
                        tracing::error!("[Sg.Plugin.Status] Status server accept error: {:?}", e);
                        continue 'accept_loop;
                    }
                }
            }
        };
        connector.serve_connection(
            TokioIo::new(stream),
            service_fn(|req| Box::pin(async move { status_plugin::create_status_html(req, gateway_name.clone(), cache_key.clone(), title.clone()) })),
        )
    }
    Ok(())
}
