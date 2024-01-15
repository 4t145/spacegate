use hyper::{header::UPGRADE, Request, Response, StatusCode};
use tardis::tokio;
use tower::BoxError;
use tower::ServiceExt;
use tower_service::Service;

use crate::service::http_client_service::get_client;
use crate::service::http_client_service::SgHttpClient;
use crate::SgBody;
use crate::SgResponseExt;
pub mod http_client_service;
pub mod ws_client_service;

/// Http backend service
/// 
/// This function could be a bottom layer of a http router, it will handle http and websocket request.
pub async fn http_backend_service(req: Request<SgBody>) -> Result<Response<SgBody>, BoxError> {
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
        Ok(resp)
    } else {
        let resp = client.request(req).await;
        Ok(resp)
    }
}
