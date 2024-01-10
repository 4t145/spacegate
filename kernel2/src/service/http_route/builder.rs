use std::{sync::Arc, time::Duration};

use tower::{filter, BoxError};

use crate::{plugin_layers::MakeSgLayer, SgBoxLayer};

use super::{match_request::SgHttpRouteMatch, SgHttpBackendLayer, SgHttpRouteLayer, SgHttpRouteRuleLayer};

#[derive(Debug, Default)]
pub struct SgHttpRouteLayerBuilder {
    pub hostnames: Vec<String>,
    pub rules: Vec<SgHttpRouteRuleLayerBuilder>,
}

impl SgHttpRouteLayerBuilder {
    pub fn new() -> Self {
        Self {
            hostnames: Vec::new(),
            rules: Vec::new(),
        }
    }
    pub fn hostnames(mut self, hostnames: impl IntoIterator<Item = String>) -> Self {
        self.hostnames = hostnames.into_iter().collect();
        self
    }
    pub fn rule(mut self, rule: SgHttpRouteRuleLayerBuilder) -> Self {
        self.rules.push(rule);
        self
    }
    pub fn build(self) -> Result<SgHttpRouteLayer, BoxError> {
        Ok(SgHttpRouteLayer {
            hostnames: self.hostnames.into(),
            rules: self.rules.into_iter().map(|b| b.build()).collect::<Result<Vec<_>, _>>()?.into(),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpRouteRuleLayerBuilder {
    r#match: SgHttpRouteMatch,
    filters: Result<Vec<SgBoxLayer>, BoxError>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayerBuilder>,
}
impl SgHttpRouteRuleLayerBuilder {
    pub fn new(r#match: SgHttpRouteMatch) -> Self {
        Self {
            r#match,
            filters: Ok(Vec::new()),
            timeouts: None,
            backends: Vec::new(),
        }
    }
    pub fn filter(mut self, filter: impl MakeSgLayer) -> Self {
        if let Ok(filters) = self.filters.as_mut() {
            let new_filter = filter.make_layer();
            match new_filter {
                Ok(new_filter) => {
                    filters.push(new_filter);
                }
                Err(e) => {
                    self.filters = Err(e.into());
                }
            }
        }
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeouts = Some(timeout);
        self
    }
    pub fn backend(mut self, backend: SgHttpBackendLayerBuilder) -> Self {
        self.backends.push(backend);
        self
    }
    pub fn build(self) -> Result<SgHttpRouteRuleLayer, BoxError> {
        Ok(SgHttpRouteRuleLayer {
            r#match: self.r#match.into(),
            filters: Arc::from(self.filters?),
            timeouts: self.timeouts,
            backends: Arc::from_iter(self.backends.into_iter().map(|b| b.build()).collect::<Result<Vec<_>, _>>()?),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpBackendLayerBuilder {
    filters: Result<Vec<SgBoxLayer>, BoxError>,
    timeout: Option<Duration>,
    weight: u16,
    backend: Result<SgBoxLayer, BoxError>,
}

impl SgHttpBackendLayerBuilder {
    pub fn new(backend: impl MakeSgLayer) -> Self {
        Self {
            filters: Ok(Vec::new()),
            timeout: None,
            weight: 1,
            backend: backend.make_layer().map_err(Into::into),
        }
    }
    pub fn filter(mut self, filter: impl MakeSgLayer) -> Self {
        if let Ok(filters) = self.filters.as_mut() {
            let new_filter = filter.make_layer();
            match new_filter {
                Ok(new_filter) => {
                    filters.push(new_filter);
                }
                Err(e) => {
                    self.filters = Err(e.into());
                }
            }
        }
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
    pub fn build(self) -> Result<SgHttpBackendLayer, BoxError> {
        Ok(SgHttpBackendLayer {
            // filters: self.filters?,
            timeout: self.timeout,
            weight: self.weight,
            client: self.backend?,
        })
    }
}
