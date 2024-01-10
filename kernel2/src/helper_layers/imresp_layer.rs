use tower::BoxError;
use tower_layer::Layer;
use tower_service::Service;

use crate::{SgBoxService, SgRequest, SgResponse};

pub struct ImmediatelyResponseLayer;

impl Layer<SgBoxService> for ImmediatelyResponseLayer
{
    type Service = ImmediatelyResponseService;

    fn layer(&self, inner: SgBoxService) -> Self::Service {
        ImmediatelyResponseService {
            inner_service: SgBoxService::new(inner),
        }
    }
}

pub struct ImmediatelyResponseService {
    inner_service: SgBoxService,
}

impl Service<Result<SgRequest, SgResponse>> for ImmediatelyResponseService {
    type Response = SgResponse;

    type Error = BoxError;

    type Future = <SgBoxService as Service<SgRequest>>::Future;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner_service.poll_ready(cx)
    }

    fn call(&mut self, req: Result<SgRequest, SgResponse>) -> Self::Future {
        match req {
            Ok(req) => self.inner_service.call(req),
            Err(resp) => Box::pin(async { Ok(resp) }),
        }
    }
}
