use crate::SgBoxLayer;

pub mod header_modifier;
pub mod inject;
pub mod rate_limit;
pub mod redirect;
pub mod retry;
// pub mod comde;

pub trait MakeSgLayer {
    type Error: std::error::Error + Send + Sync + 'static;
    fn make_layer(&self) -> Result<SgBoxLayer, Self::Error>;
}
