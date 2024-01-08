use std::{
    borrow::Cow,
    future::{self, Future},
    pin::Pin,
    sync::Arc,
    task::ready,
    time::Duration,
};

use crate::{SgFilter, SgRequest, SgResponse};
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    futures_util::FutureExt,
    log,
    rand::{self, Rng},
    tokio::{self, time::Sleep},
};
use tower::retry::{Policy, Retry as TowerRetry, RetryLayer};
use tower_layer::{Layer};

pub struct Retry {
    inner_layer: RetryLayer<RetryPolicy>
}

impl<I, O> SgFilter<I, O> for Retry
where
    I: Send + 'static,
    O: Send + 'static,
{
    type FutureReq = Pin<Box<dyn Future<Output = Result<SgRequest<I>, SgResponse<O>>> + Send>>;
    type FutureResp = Pin<Box<dyn Future<Output = TardisResult<SgResponse<O>>> + Send>>;
    fn code(&self) -> Cow<'static, str> {
        "retry".into()
    }

    fn on_req(&self, req: SgRequest<I>) -> Self::FutureReq {
        Box::pin(async move { Ok(req) })
    }

    fn on_resp(&self, resp: SgResponse<O>) -> Self::FutureResp {
        Box::pin(async move { Ok(resp) })
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub enum BackOff {
    /// Fixed interval
    Fixed,
    /// In the exponential backoff strategy, the initial delay is relatively short,
    /// but it gradually increases as the number of retries increases.
    /// Typically, the delay time is calculated by multiplying a base value with an exponential factor.
    /// For example, the delay time might be calculated as `base_value * (2 ^ retry_count)`.
    #[default]
    Exponential,
    Random,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RetryConfig {
    pub retries: u16,
    pub retryable_methods: Vec<String>,
    /// Backoff strategies can vary depending on the specific implementation and requirements.
    /// see [BackOff]
    pub backoff: BackOff,
    /// milliseconds
    pub base_interval: u64,
    /// milliseconds
    pub max_interval: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            retries: 3,
            retryable_methods: vec!["*".to_string()],
            backoff: BackOff::default(),
            base_interval: 100,
            //10 seconds
            max_interval: 10000,
        }
    }
}

#[derive(Clone)]
pub struct RetryPolicy {
    times: usize,
    config: Arc<RetryConfig>,
}
pin_project! {
    pub struct Delay<T> {
        value: Option<T>,
        #[pin]
        sleep: Sleep,
    }
}

impl<T> Delay<T> {
    pub fn new(value: T, duration: Duration) -> Self {
        Self {
            value: Some(value),
            sleep: tokio::time::sleep(duration),
        }
    }
}

impl<T> Future for Delay<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.project();
        ready!(this.sleep.poll(cx));
        std::task::Poll::Ready(this.value.take().expect("poll after ready"))
    }
}

impl<I, O> Policy<SgRequest<I>, SgResponse<O>, TardisError> for RetryPolicy
where I: Clone
{
    type Future = Delay<Self>;

    fn retry(&self, _req: &SgRequest<I>, result: Result<&SgResponse<O>, &TardisError>) -> Option<Self::Future> {
        if self.times < self.config.retries.into() && result.is_err() {
            let delay = match self.config.backoff {
                BackOff::Fixed => self.config.base_interval,
                BackOff::Exponential => self.config.base_interval * 2u64.pow(self.times as u32),
                BackOff::Random => {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(self.config.base_interval..self.config.max_interval)
                }
            };
            Some(Delay::new(
                RetryPolicy {
                    times: self.times + 1,
                    config: self.config.clone(),
                },
                Duration::from_millis(delay),
            ))
        } else {
            None
        }
    }

    fn clone_request(&self, req: &SgRequest<I>) -> Option<SgRequest<I>> {
        Some(req.clone())
    }
}
