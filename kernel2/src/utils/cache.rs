use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use tardis::{
    basic::{error::TardisError, result::TardisResult},
    cache::cache_client::TardisCacheClient,
    config::config_dto::CacheModuleConfig,
    tokio::sync::RwLock,
};

use crate::context::SgContext;

pub fn cache_clients() -> &'static RwLock<HashMap<String, Arc<TardisCacheClient>>> {
    static CACHE_CLIENTS: OnceLock<RwLock<HashMap<String, Arc<TardisCacheClient>>>> = OnceLock::new();
    CACHE_CLIENTS.get_or_init(Default::default)
}

pub async fn init(name: impl Into<String>, url: &str) -> TardisResult<()> {
    let cache = TardisCacheClient::init(&CacheModuleConfig::builder().url(url.parse().expect("invalid url")).build()).await?;
    {
        let mut write = cache_clients().write().await;
        write.insert(name.into(), Arc::new(cache));
    }
    Ok(())
}

pub async fn remove(name: &str) -> TardisResult<()> {
    {
        let mut write = cache_clients().write().await;
        write.remove(name);
    }
    Ok(())
}

pub async fn get(name: &str) -> TardisResult<Arc<TardisCacheClient>> {
    {
        let read = cache_clients().read().await;
        read.get(name).cloned().ok_or_else(|| TardisError::bad_request("[SG.server] Get client failed", ""))
    }
}

impl SgContext {
    pub async fn cache(&self) -> TardisResult<std::sync::Arc<tardis::cache::cache_client::TardisCacheClient>> {
        get(&self.gateway_name).await
    }
}
