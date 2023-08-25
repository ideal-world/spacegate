use async_trait::async_trait;
use http::header;
use hyper::{Body, body::Bytes};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns, futures_util,
};

use crate::plugins::context::SgRouteFilterRequestAction;

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

pub const CODE: &str = "maintenance";
pub struct SgFilterMaintenanceDef;

impl SgPluginFilterDef for SgFilterMaintenanceDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterMaintenance>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterMaintenance {
    is_enabled: bool,
    title: String,
    msg: String,
}

impl Default for SgFilterMaintenance {
    fn default() -> Self {
        Self {
            is_enabled: false,
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
        if self.is_enabled {
            ctx.set_action(SgRouteFilterRequestAction::Response);
            let request_headers = ctx.request.get_headers();
            let content_type = request_headers.get(header::CONTENT_TYPE).map(|content_type| content_type.to_str().unwrap_or("").split(',').collect_vec()).unwrap_or_default();
            let accept_type = request_headers.get(header::ACCEPT).map(|accept| accept.to_str().unwrap_or("").split(',').collect_vec()).unwrap_or_default();

            if content_type.contains(&"text/html") || accept_type.contains(&"text/html") {
                let title = self.title.clone();
                let msg = self.msg.clone().replace("/n", "<br>");
                ctx.response.set_header(header::CONTENT_TYPE.as_ref(), "text/html")?;
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
                let resp_body = Body::from(Bytes::from(body));
                ctx.response.replace_body(resp_body);
            } else if content_type.contains(&"application/json") || accept_type.contains(&"application/json") {
                let msg = self.msg.clone();
                return Err(TardisError::forbidden(&msg, ""));
            } else {
                ctx.response.set_body("<h1>Maintenance</h1>".to_string().into_bytes())?;
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}
