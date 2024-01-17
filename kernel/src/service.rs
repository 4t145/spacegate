use tardis::basic::result::TardisResult;

use crate::config::gateway_dto::SgGateway;

impl SgGateway {
    pub async fn start(&self) -> TardisResult<()> {

        self.listeners
    }
}