use serde::{Deserialize, Serialize};
use regex::Regex;

use crate::{utils::query_kv::QueryKvIter, Request, SgBody};

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
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match self {
            SgHttpPathMatch::Exact(path) => req.uri().path() == path,
            SgHttpPathMatch::Prefix(path) => req.uri().path().starts_with(path),
            SgHttpPathMatch::Regular(path) => path.is_match(req.uri().path()),
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
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        match &self.policy {
            SgHttpHeaderMatchPolicy::Exact(header) => req.headers().get(&self.name).is_some_and(|v| v == header),
            SgHttpHeaderMatchPolicy::Regular(header) => req.headers().iter().any(|(k, v)| k.as_str() == self.name && v.to_str().map_or(false, |v| header.is_match(v))),
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
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        let query = req.uri().query();
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
#[serde(transparent)]

pub struct SgHttpMethodMatch(String);

impl MatchRequest for SgHttpMethodMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        req.method().as_str().eq_ignore_ascii_case(&self.0)
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SgHttpRouteMatch {
    pub path: Option<SgHttpPathMatch>,
    pub header: Option<Vec<SgHttpHeaderMatch>>,
    pub query: Option<Vec<SgHttpQueryMatch>>,
    pub method: Option<Vec<SgHttpMethodMatch>>,
}

pub trait MatchRequest {
    fn match_request(&self, req: &Request<SgBody>) -> bool;
}

impl MatchRequest for SgHttpRouteMatch {
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.path.match_request(req) && self.header.match_request(req) && self.query.match_request(req) && self.method.match_request(req)
    }
}

impl<T> MatchRequest for Option<T>
where
    T: MatchRequest,
{
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.as_ref().map(|r| MatchRequest::match_request(r, req)).unwrap_or(true)
    }
}

impl<T> MatchRequest for Vec<T>
where
    T: MatchRequest,
{
    fn match_request(&self, req: &Request<SgBody>) -> bool {
        self.iter().any(|query| query.match_request(req))
    }
}
