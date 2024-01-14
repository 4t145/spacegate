use std::{sync::Arc, time::Duration};

use tower::{filter, BoxError};

use crate::{plugin_layers::MakeSgLayer, SgBoxLayer};

use super::{match_request::SgHttpRouteMatch, SgHttpBackendLayer, SgHttpRouteLayer, SgHttpRouteRuleLayer};

#[derive(Debug)]
pub struct SgHttpRouteLayerBuilder {
    pub hostnames: Vec<String>,
    pub rules: Vec<SgHttpRouteRuleLayerBuilder>,
    pub fallback: Result<SgHttpRouteRuleLayer, BoxError>,
}

impl Default for SgHttpRouteLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgHttpRouteLayerBuilder {
    pub fn new() -> Self {
        Self {
            hostnames: Vec::new(),
            rules: Vec::new(),
            fallback: Err(BoxError::from("No fallback route specified")),
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
    pub fn fallback(mut self, fallback: SgHttpRouteRuleLayerBuilder) -> Self {
        self.fallback = fallback.build();
        self
    }
    pub fn build(self) -> Result<SgHttpRouteLayer, BoxError> {
        let mut rules = vec![self.fallback?];
        for b in self.rules.into_iter() {
            rules.push(b.build()?)
        }
        Ok(SgHttpRouteLayer {
            hostnames: self.hostnames.into(),
            rules: rules.into(),
            fallback_index: 0,
        })
    }
}

#[derive(Debug)]
pub struct SgHttpRouteRuleLayerBuilder {
    r#match: Option<SgHttpRouteMatch>,
    filters: Result<Vec<SgBoxLayer>, BoxError>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayerBuilder>,
}
impl Default for SgHttpRouteRuleLayerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SgHttpRouteRuleLayerBuilder {
    pub fn new() -> Self {
        Self {
            r#match: None,
            filters: Ok(Vec::new()),
            timeouts: None,
            backends: Vec::new(),
        }
    }
    pub fn r#match(mut self, r#match: SgHttpRouteMatch) -> Self {
        self.r#match = Some(r#match);
        self
    }
    pub fn match_all(mut self) -> Self {
        self.r#match = None;
        self
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
    plugins: Result<Vec<SgBoxLayer>, BoxError>,
    timeout: Option<Duration>,
    weight: u16,
}

impl Default for SgHttpBackendLayerBuilder {
    fn default() -> Self {
        Self {
            plugins: Ok(Vec::new()),
            timeout: None,
            weight: 1,
        }
    }
}

impl SgHttpBackendLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn plugin(mut self, filter: impl MakeSgLayer) -> Self {
        if let Ok(plugins) = self.plugins.as_mut() {
            let new_plugin = filter.make_layer();
            match new_plugin {
                Ok(new_filter) => {
                    plugins.push(new_filter);
                }
                Err(e) => {
                    self.plugins = Err(e.into());
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
            filters: Arc::from(self.plugins?),
            timeout: self.timeout,
            weight: self.weight,
        })
    }
}
