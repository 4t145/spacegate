use async_trait::async_trait;
use http::header;
use std::ops::Range;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tardis::chrono::{Local, NaiveTime};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use crate::plugins::context::SgRouteFilterRequestAction;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

pub const CODE: &str = "maintenance";
pub struct SgFilterMaintenanceDef;

impl SgPluginFilterDef for SgFilterMaintenanceDef {
    fn get_code(&self) -> &'static str {
        CODE
    }
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterMaintenance>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterMaintenance {
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    title: String,
    msg: String,
}

impl SgFilterMaintenance {
    pub fn check_by_time(&self, time: NaiveTime) -> bool {
        let contains_time = |range: &Range<NaiveTime>| {
            if range.start > range.end {
                !(range.end..range.start).contains(&time)
            } else {
                range.contains(&time)
            }
        };
        if let Some(enabled_time) = &self.enabled_time_range {
            enabled_time.iter().any(contains_time)
        } else {
            true
        }
    }

    pub fn check_by_now(&self) -> bool {
        let local_time = Local::now().time();
        self.check_by_time(local_time)
    }
}

impl Default for SgFilterMaintenance {
    fn default() -> Self {
        Self {
            enabled_time_range: None,
            title: "System Maintenance".to_string(),
            msg: "We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly./n Thank you for your patience, understanding, and support.".to_string(),
        }
    }
}

#[async_trait]
impl SgPluginFilter for SgFilterMaintenance {
    fn accept(&self) -> super::SgPluginFilterAccept {
        super::SgPluginFilterAccept {
            kind: vec![super::SgPluginFilterKind::Http],
            ..Default::default()
        }
    }
    async fn init(&mut self, _: &SgPluginFilterInitDto) -> TardisResult<()> {
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if self.check_by_now() {
            ctx.set_action(SgRouteFilterRequestAction::Response);
            let request_headers = ctx.request.get_headers();
            let content_type = request_headers.get(header::CONTENT_TYPE).map(|content_type| content_type.to_str().unwrap_or("").split(',').collect_vec()).unwrap_or_default();
            let accept_type = request_headers.get(header::ACCEPT).map(|accept| accept.to_str().unwrap_or("").split(',').collect_vec()).unwrap_or_default();

            if content_type.contains(&"text/html") || accept_type.contains(&"text/html") {
                let title = self.title.clone();
                let msg = self.msg.clone().replace("/n", "<br>");
                ctx.response.set_header(header::CONTENT_TYPE, "text/html")?;
                let body = format!(
                    r##"<!DOCTYPE html>
                <html>
                <head>
                    <meta charset="UTF-8" />
                    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                    <meta http-equiv="cache-control" content="no-cache, no-store, must-revalidate" />
                    <title>{title}</title>
                    <style>
                        body {{
                            background: radial-gradient(circle at top left, #FFD700 0%, #FF8C00 25%, #FF4500 50%, #FF6347 75%, #FF1493 100%);
                            height: 100vh;
                            display: flex;
                            justify-content: center;
                            align-items: center;
                        }}
                
                        h1 {{
                            font-size: 40px;
                            color: #FFFFFF;
                        }}
                
                        p {{
                            font-size: 20px;
                            color: #FFFFFF;
                            margin-bottom: 20px;
                        }}
                    </style>
                </head>
                <body>
                    <div>
                    <h1>{title}</h1>
                    <br>
                        <p>{msg}</p>
                    </div>
                </body>
                </body>
                </html>
                "##
                );
                ctx.response.set_body(body);
            } else if content_type.contains(&"application/json") || accept_type.contains(&"application/json") {
                let msg = self.msg.clone();
                return Err(TardisError::forbidden(&msg, ""));
            } else {
                ctx.response.set_body(format!("<h1>{}</h1>", self.title));
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}
