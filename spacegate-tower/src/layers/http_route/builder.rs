use std::{sync::Arc, time::Duration};

use tower::BoxError;

use crate::{plugin_layers::MakeSgLayer, SgBoxLayer};

use super::{match_request::SgHttpRouteMatch, SgHttpBackendLayer, SgHttpRoute, SgHttpRouteRuleLayer};

#[derive(Debug)]
pub struct SgHttpRouteLayerBuilder {
    pub hostnames: Vec<String>,
    pub rules: Vec<SgHttpRouteRuleLayer>,
    pub plugins: Result<Vec<SgBoxLayer>, BoxError>,
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
            plugins: Ok(Vec::new()),
            fallback: Err(BoxError::from("No fallback route specified")),
        }
    }
    pub fn hostnames(mut self, hostnames: impl IntoIterator<Item = String>) -> Self {
        self.hostnames = hostnames.into_iter().collect();
        self
    }
    pub fn rule(mut self, rule: SgHttpRouteRuleLayer) -> Self {
        self.rules.push(rule);
        self
    }
    pub fn plugin(mut self, plugin: impl MakeSgLayer) -> Self {
        if let Ok(plugins) = self.plugins.as_mut() {
            let new_plugin = plugin.make_layer();
            match new_plugin {
                Ok(new_plugin) => {
                    plugins.push(new_plugin);
                }
                Err(e) => {
                    self.plugins = Err(e);
                }
            }
        }
        self
    }
    pub fn build(self) -> Result<SgHttpRoute, BoxError> {
        let mut rules = vec![self.fallback?];
        for r in self.rules.into_iter() {
            rules.push(r);
        }
        Ok(SgHttpRoute {
            plugins: Arc::from(self.plugins?),
            hostnames: self.hostnames.into(),
            rules: rules.into(),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpRouteRuleLayerBuilder {
    r#match: Option<SgHttpRouteMatch>,
    plugins: Result<Vec<SgBoxLayer>, BoxError>,
    timeouts: Option<Duration>,
    backends: Vec<SgHttpBackendLayer>,
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
            plugins: Ok(Vec::new()),
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
    pub fn plugin(mut self, plugin: impl MakeSgLayer) -> Self {
        if let Ok(plugins) = self.plugins.as_mut() {
            let new_plugin = plugin.make_layer();
            match new_plugin {
                Ok(new_plugin) => {
                    plugins.push(new_plugin);
                }
                Err(e) => {
                    self.plugins = Err(e.into());
                }
            }
        }
        self
    }
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeouts = Some(timeout);
        self
    }
    pub fn backend(mut self, backend: SgHttpBackendLayer) -> Self {
        self.backends.push(backend);
        self
    }
    pub fn build(self) -> Result<SgHttpRouteRuleLayer, BoxError> {
        Ok(SgHttpRouteRuleLayer {
            r#match: self.r#match.into(),
            plugins: Arc::from(self.plugins?),
            timeouts: self.timeouts,
            backends: Arc::from_iter(self.backends),
        })
    }
}

#[derive(Debug)]
pub struct SgHttpBackendLayerBuilder {
    host: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    plugins: Result<Vec<SgBoxLayer>, BoxError>,
    timeout: Option<Duration>,
    weight: u16,
}

impl Default for SgHttpBackendLayerBuilder {
    fn default() -> Self {
        Self {
            host: None,
            port: None,
            protocol: None,
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
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }
    pub fn protocol(mut self, protocol: String) -> Self {
        self.protocol = Some(protocol);
        self
    }
    pub fn build(self) -> Result<SgHttpBackendLayer, BoxError> {
        Ok(SgHttpBackendLayer {
            host: self.host.map(Into::into),
            port: self.port,
            scheme: None,
            filters: Arc::from(self.plugins?),
            timeout: self.timeout,
            weight: self.weight,
        })
    }
}
