use std::sync::Arc;

use hyper::{Response, StatusCode};
use tardis::basic::error::TardisError;
use tower::{filter::Predicate, BoxError};

use crate::{ReqOrResp, SgBody, SgRequest, SgResponse};

#[derive(Debug, Clone)]
pub struct FilterByHostnames {
    pub hostnames: Arc<[String]>,
}

impl FilterByHostnames {
    pub fn check(&mut self, request: SgRequest) -> ReqOrResp {
        if self.hostnames.is_empty() {
            Ok(request)
        } else {
            let hostname = request.request.uri().host();
            if let Some(hostname) = hostname {
                if self.hostnames.iter().any(|h| h == hostname) {
                    Ok(request)
                } else {
                    Err(SgResponse::with_code_message(request.context, StatusCode::FORBIDDEN, "hostname not allowed"))
                }
            } else {
                Err(SgResponse::with_code_message(request.context, StatusCode::FORBIDDEN, "missing hostname"))
            }
        }
    }
}

impl Predicate<SgRequest> for FilterByHostnames {
    type Request = ReqOrResp;

    fn check(&mut self, request: SgRequest) -> Result<ReqOrResp, BoxError> {
        Ok(FilterByHostnames::check(self, request))
    }
}
