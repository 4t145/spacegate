use std::convert::Infallible;

use hyper::{header::UPGRADE, Request, Response, StatusCode};
use tower::BoxError;
use tower::ServiceExt;
use tower_service::Service;
use tracing::instrument;

use crate::service::http_client_service::get_client;
use crate::SgBody;
use crate::SgBoxService;
use crate::SgResponseExt;

pub mod echo;
pub mod http_client_service;
pub mod ws_client_service;

/// Http backend service
///
/// This function could be a bottom layer of a http router, it will handle http and websocket request.
///
/// This can handle both websocket connection and http request.
pub async fn http_backend_service_inner(req: Request<SgBody>) -> Result<Response<SgBody>, BoxError> {
    tracing::trace!("start a backend request");
    let mut client = get_client();
    if let Some(upgrade) = req.headers().get(UPGRADE) {
        if !upgrade.as_bytes().eq_ignore_ascii_case(b"websocket") {
            return Ok(Response::with_code_message(StatusCode::NOT_IMPLEMENTED, "[Sg.Websocket] unsupported upgrade protocol"));
        }
        let (part, body) = req.into_parts();
        let body = body.dump().await?;
        let req = Request::from_parts(part, body);
        let resp = client.ready().await?.call(req.clone()).await?;
        let (part, body) = resp.into_parts();
        let body = body.dump().await?;
        let resp = Response::from_parts(part, body);
        let req_for_upgrade = req.clone();
        let resp_for_upgrade = resp.clone();
        tokio::task::spawn(async move {
            let (s, c) = futures_util::join!(hyper::upgrade::on(req_for_upgrade), hyper::upgrade::on(resp_for_upgrade));
            let upgrade_as_server = s?;
            let upgrade_as_client = c?;
            ws_client_service::service(upgrade_as_server, upgrade_as_client).await?;
            <Result<(), BoxError>>::Ok(())
        });
        tracing::trace!(upgrade = true, "finish backend request");
        Ok(resp)
    } else {
        tracing::trace!(upgrade = false, "finish backend request");
        let resp = client.request(req).await;
        Ok(resp)
    }
}

#[instrument]
pub async fn http_backend_service(req: Request<SgBody>) -> Result<Response<SgBody>, Infallible> {
    match http_backend_service_inner(req).await {
        Ok(resp) => Ok(resp),
        Err(err) => Ok(Response::with_code_message(StatusCode::INTERNAL_SERVER_ERROR, format!("[Sg.Client] Client error: {err}"))),
    }
}

pub fn get_http_backend_service() -> SgBoxService {
    SgBoxService::new(tower::util::service_fn(http_backend_service))
}

pub fn get_echo_service() -> SgBoxService {
    SgBoxService::new(tower::util::service_fn(echo::echo))
}
