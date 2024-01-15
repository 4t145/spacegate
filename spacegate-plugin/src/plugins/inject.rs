use hyper::header::HeaderName;
#[derive(Default, Debug, Clone)]
pub struct SgFilterInject {
    pub req_inject_url: Option<String>,
    pub req_timeout_ms: Option<u64>,
    pub resp_inject_url: Option<String>,
    pub resp_timeout_ms: Option<u64>,
}

// those headers interior mutable
#[allow(clippy::declare_interior_mutable_const)]
const SG_INJECT_REAL_METHOD: HeaderName = HeaderName::from_static("sg-inject-real-method");
#[allow(clippy::declare_interior_mutable_const)]
const SG_INJECT_REAL_URL: HeaderName = HeaderName::from_static("sg-inject-real-url");

