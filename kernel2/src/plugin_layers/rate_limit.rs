use std::{
    convert::Infallible,
    future::{ready, Future, Ready},
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};

use hyper::StatusCode;
use tardis::{cache::Script, tardis_static};

use crate::{
    helper_layers::bidirection_filter_layer::{Bdf, BdfLayer, BdfService},
    SgRequest, SgResponse,
};

use super::MakeSgLayer;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_request_number: Option<u64>,
    pub time_window_ms: Option<u64>,
}

const CONF_LIMIT_KEY: &str = "sg:plugin:filter:limit:";

tardis_static! {
    /// Flow limit script
    ///
    /// # Arguments
    ///
    /// * KEYS[1]  counter key
    /// * KEYS[2]  last counter reset timestamp key
    /// * ARGV[1]  maximum number of request
    /// * ARGV[2]  time window
    /// * ARGV[3]  current timestamp
    ///
    /// # Return
    ///
    /// * 1   passed
    /// * 0   limited
    ///
    /// # Kernel logic
    ///
    /// ```lua
    /// -- Use `counter` to accumulate 1 for each request
    /// local current_count = tonumber(redis.call('incr', KEYS[1]));
    /// if current_count == 1 then
    ///     -- The current request is the first request, record the current timestamp
    ///     redis.call('set', KEYS[2], ARGV[3]);
    /// end
    /// -- When the `counter` value reaches the maximum number of requests
    /// if current_count > tonumber(ARGV[1]) then
    ///     local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
    ///     if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
    ///          -- Last reset time + time window > current time,
    ///          -- indicating that the request has reached the upper limit within this time period,
    ///          -- so the request is limited
    ///         return 0;
    ///     end
    ///     -- Otherwise reset the counter and timestamp,
    ///     -- and allow the request
    ///     redis.call('set', KEYS[1], '1')
    ///     redis.call('set', KEYS[2], ARGV[3]);
    /// end
    /// return 1;
    /// ```
    pub script: Script = Script::new(
        r"
    local current_count = tonumber(redis.call('incr', KEYS[1]));
    if current_count == 1 then
        redis.call('set', KEYS[2], ARGV[3]);
    end
    if current_count > tonumber(ARGV[1]) then
        local last_refresh_time = tonumber(redis.call('get', KEYS[2]));
        if last_refresh_time + tonumber(ARGV[2]) > tonumber(ARGV[3]) then
            return 0;
        end
        redis.call('set', KEYS[1], '1')
        redis.call('set', KEYS[2], ARGV[3]);
    end
    return 1;
    ",
    );
}

impl RateLimitConfig {
    async fn req_filter(&self, id: &str, req: SgRequest) -> Result<SgRequest, SgResponse> {
        if let Some(max_request_number) = &self.max_request_number {
            let result: &bool = &script()
                // counter key
                .key(format!("{CONF_LIMIT_KEY}{id}"))
                // last counter reset timestamp key
                .key(format!("{CONF_LIMIT_KEY}{id}_ts"))
                // maximum number of request
                .arg(max_request_number)
                // time window
                .arg(self.time_window_ms.unwrap_or(1000))
                // current timestamp
                .arg(SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("invalid system time: before unix epoch").as_millis() as u64)
                .invoke_async(&mut req.context.cache().await?.cmd().await.map_err(SgResponse::internal_error(req.context.clone()))?)
                .await
                .map_err(|e| SgResponse::with_code_message(req.context.clone(), StatusCode::INTERNAL_SERVER_ERROR, format!("[SG.Filter.Limit] redis error: {e}")))?;

            if !result {
                return Err(SgResponse::with_code_message(
                    req.context.clone(),
                    StatusCode::TOO_MANY_REQUESTS,
                    "[SG.Filter.Limit] too many requests",
                ));
            }
        }
        Ok(req)
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitFilter {
    pub config: Arc<RateLimitConfig>,
    pub service_id: Arc<str>,
}

impl Bdf for RateLimitFilter {
    type FutureReq = Pin<Box<dyn Future<Output = Result<SgRequest, SgResponse>> + Send + 'static>>;

    type FutureResp = Ready<SgResponse>;

    fn on_req(&self, req: SgRequest) -> Self::FutureReq {
        let config = self.config.clone();
        let service_id = self.service_id.clone();
        Box::pin(async move { config.req_filter(&service_id, req).await })
    }

    fn on_resp(&self, resp: SgResponse) -> Self::FutureResp {
        ready(resp)
    }
}

pub type RateLimitLayer = BdfLayer<RateLimitFilter>;
pub type RateLimitService<S> = BdfService<RateLimitFilter, S>;

impl MakeSgLayer for (RateLimitConfig, String) {
    type Error = Infallible;

    fn make_layer(&self) -> Result<crate::SgBoxLayer, Self::Error> {
        let service_id = Arc::from(self.1.as_str());
        let config = Arc::new(self.0.clone());
        Ok(crate::SgBoxLayer::new(BdfLayer::new(RateLimitFilter {
            config,
            service_id,
        })))
    }
}
