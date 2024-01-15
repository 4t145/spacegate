use std::{fmt, collections::HashMap, sync::Arc};
use hyper::{http, Response};
use serde::{Serialize, Deserialize};
use tardis::{basic::{error::TardisError, result::TardisResult}, log, url::Url};

use crate::{SgBody, route_layers::http_route::match_request::SgHttpRouteMatch};

// TODO
/// The SgPluginFilterKind enum is used to represent the types of plugins
/// supported by Spacegate or to identify the type of the current request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgPluginFilterKind {
    Http,
    Grpc,
    Ws,
}

/// The SgAttachedLevel enum is used to represent the levels at which a plugin
/// can be attached within
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SgAttachedLevel {
    Gateway,
    HttpRoute,
    Rule,
    Backend,
}

impl fmt::Display for SgAttachedLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SgAttachedLevel::Gateway => write!(f, "GateWay"),
            SgAttachedLevel::HttpRoute => write!(f, "HttpRoute"),
            SgAttachedLevel::Rule => write!(f, "Rule"),
            SgAttachedLevel::Backend => write!(f, "Backend"),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct SgContext {
    /// A unique identifier for the request.
    request_id: String,
    headers: http::HeaderMap,
    matched: Option<Arc<SgHttpRouteMatch>>,
    /// see [SgPluginFilterKind]
    // request_kind: SgPluginFilterKind,

    // chosen_route_rule: Option<ChosenHttpRouteRuleInst>,
    // chosen_backend: Option<AvailableBackendInst>,

    backend: Option<String>,

    ext: HashMap<String, String>,

    /// Describe user information
    ident_info: Option<SGIdentInfo>,
    // action: SgRouteFilterRequestAction,
    pub gateway_name: String,
}

impl SgContext {
    pub fn internal_error() -> Self {
        Self::default()
    }
}


#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SGIdentInfo {
    pub id: String,
    pub name: Option<String>,
    pub roles: Vec<SGRoleInfo>,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SGRoleInfo {
    pub id: String,
    pub name: Option<String>,
}