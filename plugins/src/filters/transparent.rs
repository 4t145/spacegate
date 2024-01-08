use std::{borrow::Cow, future::Future, pin::Pin};

use tardis::basic::result::TardisResult;

use crate::{SgFilter, SgRequest, SgResponse};

pub struct Transparent;

impl<I, O> SgFilter<I, O> for Transparent 
where
    I: Send + 'static,
    O: Send + 'static,
{
    type FutureReq = Pin<Box<dyn Future<Output = Result<SgRequest<I>, SgResponse<O>>> + Send>>;
    type FutureResp = Pin<Box<dyn Future<Output = TardisResult<SgResponse<O>>> + Send>>;
    fn code(&self) -> Cow<'static, str> {
        "Transparent".into()
    }

    fn on_req(&self, req: SgRequest<I>) -> Self::FutureReq {
        Box::pin(async move { Ok(req) })
    }

    fn on_resp(&self, resp: SgResponse<O>) -> Self::FutureResp {
        Box::pin(async move { Ok(resp) })
    }
}
