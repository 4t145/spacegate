use std::sync::{Arc, OnceLock};

use futures_util::future::BoxFuture;
use tokio::sync::RwLock;
use tower_layer::Layer;

#[derive(Default, Debug, Clone)]
pub struct ReloadLayer<S> {
    pub reloader: Reloader<S>,
}

impl<S> Layer<S> for ReloadLayer<S>
where
    S: Send,
{
    type Service = Reload<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Reload {
            service: Arc::new(RwLock::new(inner)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reload<S>
where
    S: Send,
{
    pub(crate) service: Arc<RwLock<S>>,
}

#[derive(Debug)]
pub struct Reloader<S> {
    pub service: OnceLock<Arc<RwLock<S>>>,
}

impl<S> Default for Reloader<S> {
    fn default() -> Self {
        Self { service: OnceLock::new() }
    }
}

impl<S> Clone for Reloader<S> {
    fn clone(&self) -> Self {
        Self { service: self.service.clone() }
    }
}

impl<S> Reloader<S> {
    pub fn setup(&self, service: Arc<RwLock<S>>) {
        self.service.set(service);
    }
    pub async fn reload(&self, service: S) {
        if let Some(wg) = self.service.get() {
            let mut wg = wg.write().await;
            *wg = service;
        } else {
            tracing::warn!("reloader not initialized");
        }
    }
    pub fn into_layer(self) -> ReloadLayer<S> {
        ReloadLayer { reloader: self }
    }
}

impl<Request, S> hyper::service::Service<Request> for Reload<S>
where
    Request: Send + Sync + 'static,
    S: hyper::service::Service<Request> + Send + Sync + 'static,
    <S as hyper::service::Service<Request>>::Future: std::marker::Send,
{
    type Response = S::Response;

    type Error = S::Error;

    type Future = BoxFuture<'static, Result<S::Response, S::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let service = self.service.clone();
        Box::pin(async move {
            let rg = service.read_owned().await;
            let fut = rg.call(req);
            drop(rg);
            fut.await
        })
    }
}
