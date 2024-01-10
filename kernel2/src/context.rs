use std::{fmt, collections::HashMap};
use hyper::{http, Response};
use serde::{Serialize, Deserialize};
use tardis::{basic::{error::TardisError, result::TardisResult}, log, url::Url};

use crate::{SgResponse, SgBody};


// pub fn http_common_modify_path(
//     uri: &hyper::http::Uri,
//     modify_path: &Option<SgHttpPathModifier>,
//     matched_match_inst: Option<&SgHttpRouteMatchInst>,
// ) -> TardisResult<Option<http::Uri>> {
//     if let Some(modify_path) = &modify_path {
//         let mut uri = Url::parse(&uri.to_string())?;
//         match modify_path.kind {
//             SgHttpPathModifierType::ReplaceFullPath => {
//                 log::debug!(
//                     "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplaceFullPath], form {} to  {}",
//                     uri.path(),
//                     modify_path.value
//                 );
//                 uri.set_path(&modify_path.value);
//             }
//             SgHttpPathModifierType::ReplacePrefixMatch => {
//                 if let Some(Some(matched_path)) = matched_match_inst.map(|m| m.path.as_ref()) {
//                     match matched_path.kind {
//                         SgHttpPathMatchType::Exact => {
//                             // equivalent to ` SgHttpPathModifierType::ReplaceFullPath`
//                             // https://cloud.yandex.com/en/docs/application-load-balancer/k8s-ref/http-route
//                             log::debug!(
//                                 "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Exact], form {} to {}",
//                                 uri.path(),
//                                 modify_path.value
//                             );
//                             uri.set_path(&modify_path.value);
//                         }
//                         _ => {
//                             let origin_path = uri.path();
//                             let match_path = if matched_path.kind == SgHttpPathMatchType::Prefix {
//                                 &matched_path.value
//                             } else {
//                                 // Support only one capture group
//                                 matched_path.regular.as_ref().expect("").captures(origin_path).map(|cap| cap.get(1).map_or("", |m| m.as_str())).unwrap_or("")
//                             };
//                             let match_path_reduce = origin_path.strip_prefix(match_path).ok_or_else(|| {
//                                 TardisError::format_error(
//                                     "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Exact] failed",
//                                     "",
//                                 )
//                             })?;
//                             let new_path = if match_path_reduce.is_empty() {
//                                 modify_path.value.to_string()
//                             } else if match_path_reduce.starts_with('/') && modify_path.value.ends_with('/') {
//                                 format!("{}{}", modify_path.value, &match_path_reduce.to_string()[1..])
//                             } else if match_path_reduce.starts_with('/') || modify_path.value.ends_with('/') {
//                                 format!("{}{}", modify_path.value, &match_path_reduce.to_string())
//                             } else {
//                                 format!("{}/{}", modify_path.value, &match_path_reduce.to_string())
//                             };
//                             log::debug!(
//                                 "[SG.Plugin.Filter.Common] Modify path with modify kind [ReplacePrefixMatch] and match kind [Prefix/Regular], form {} to {}",
//                                 origin_path,
//                                 new_path,
//                             );
//                             uri.set_path(&new_path);
//                         }
//                     }
//                 } else {
//                     // TODO
//                     // equivalent to ` SgHttpPathModifierType::ReplaceFullPath`
//                     log::debug!(
//                         "[SG.Plugin.Filter.Common] Modify path with modify kind [None], form {} to {}",
//                         uri.path(),
//                         modify_path.value,
//                     );
//                     uri.set_path(&modify_path.value);
//                 }
//             }
//         }
//         return Ok(Some(
//             uri.as_str().parse().map_err(|e| TardisError::internal_error(&format!("[SG.Plugin.Filter.Common] uri parse error: {}", e), ""))?,
//         ));
//     }
//     Ok(None)
// }

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
    /// see [SgPluginFilterKind]
    // request_kind: SgPluginFilterKind,

    // chosen_route_rule: Option<ChosenHttpRouteRuleInst>,
    // chosen_backend: Option<AvailableBackendInst>,

    ext: HashMap<String, String>,

    /// Describe user information
    ident_info: Option<SGIdentInfo>,
    // action: SgRouteFilterRequestAction,
    gateway_name: String,
}

impl SgContext {
    pub fn response(self, response: Response<SgBody>) -> SgResponse {
        SgResponse {
            context: self,
            response,
        }
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