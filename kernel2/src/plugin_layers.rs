use std::{borrow::Cow, future::Future};

use tower::BoxError;

use crate::{SgBoxLayer, SgBoxService, SgRequest, SgResponse};

pub mod header_modifier;
pub mod redirect;
pub mod retry;
pub mod inject;
pub mod rate_limit;
// pub mod comde;

pub trait MakeSgLayer {
    type Error: std::error::Error + Send + Sync + 'static;
    fn make_layer(&self) -> Result<SgBoxLayer, Self::Error>;
}
