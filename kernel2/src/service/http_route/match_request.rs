use serde::{Deserialize, Serialize};
use tardis::regex::Regex;

use crate::{utils::query_kv::QueryKvIter, SgRequest};

/// PathMatchType specifies the semantics of how HTTP paths should be compared.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SgHttpPathMatch {
    /// Matches the URL path exactly and with case sensitivity.
    Exact(String),
    /// Matches based on a URL path prefix split by /. Matching is case sensitive and done on a path element by element basis.
    /// A path element refers to the list of labels in the path split by the / separator. When specified, a trailing / is ignored.
    Prefix(String),
    /// Matches if the URL path matches the given regular expression with case sensitivity.
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

impl MatchRequest for SgHttpPathMatch {
    fn match_request(&self, req: &SgRequest) -> bool {
        match self {
            SgHttpPathMatch::Exact(path) => req.request.uri().path() == path,
            SgHttpPathMatch::Prefix(path) => req.request.uri().path().starts_with(path),
            SgHttpPathMatch::Regular(path) => path.is_match(req.request.uri().path()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SgHttpHeaderMatchPolicy {
    Exact(String),
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpHeaderMatch {
    pub name: String,
    #[serde(flatten)]
    pub policy: SgHttpHeaderMatchPolicy,
}

impl MatchRequest for SgHttpHeaderMatch {
    fn match_request(&self, req: &SgRequest) -> bool {
        match &self.policy {
            SgHttpHeaderMatchPolicy::Exact(header) => req.request.headers().get(&self.name).is_some_and(|v| v == header),
            SgHttpHeaderMatchPolicy::Regular(header) => req.request.headers().iter().any(|(k, v)| k.as_str() == self.name && v.to_str().map_or(false, |v| header.is_match(v))),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SgHttpQueryMatchPolicy {
    Exact(String),
    #[serde(with = "serde_regex")]
    Regular(Regex),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SgHttpQueryMatch {
    pub name: String,
    #[serde(flatten)]
    pub policy: SgHttpQueryMatchPolicy,
}

impl MatchRequest for SgHttpQueryMatch {
    fn match_request(&self, req: &SgRequest) -> bool {
        let query = req.request.uri().query();
        if let Some(query) = query {
            let mut iter = QueryKvIter::new(query);
            match &self.policy {
                SgHttpQueryMatchPolicy::Exact(query) => iter.any(|(k, v)| k == self.name && v == Some(query)),
                SgHttpQueryMatchPolicy::Regular(query) => iter.any(|(k, v)| k == self.name && v.map_or(false, |v| query.is_match(v))),
            }
        } else {
            false
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SgHttpRouteMatch {
    pub path: Option<SgHttpPathMatch>,
    pub header: Vec<SgHttpHeaderMatch>,
    pub query: Vec<SgHttpQueryMatch>,
    pub method: Vec<String>,
}

pub trait MatchRequest {
    fn match_request(&self, req: &SgRequest) -> bool;
}

impl MatchRequest for SgHttpRouteMatch {
    fn match_request(&self, req: &SgRequest) -> bool {
        if let Some(path) = &self.path {
            if path.match_request(req) {
                return true;
            }
        }
        if !self.header.is_empty() && self.header.iter().any(|header| header.match_request(req)) {
            return true;
        }
        if !self.query.is_empty() && self.query.iter().any(|query| query.match_request(req)) {
            return true;
        }
        if !self.method.is_empty() && self.method.iter().any(|method| method.eq_ignore_ascii_case(req.request.method().as_str())) {
            return true;
        }
        false
    }
}
