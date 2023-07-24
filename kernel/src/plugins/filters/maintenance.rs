use async_trait::async_trait;
use http::{header, HeaderValue};
use serde::{Deserialize, Serialize};
use tardis::{
    basic::{error::TardisError, result::TardisResult},
    TardisFuns,
};

use crate::{functions::http_route::SgHttpRouteMatchInst, plugins::context::SgRouteFilterRequestAction};

use super::{BoxSgPluginFilter, SgPluginFilter, SgPluginFilterDef, SgPluginFilterInitDto, SgRoutePluginContext};

pub const CODE: &str = "maintenance";
pub struct SgFilterMaintenanceDef;

impl SgPluginFilterDef for SgFilterMaintenanceDef {
    fn inst(&self, spec: serde_json::Value) -> TardisResult<BoxSgPluginFilter> {
        let filter = TardisFuns::json.json_to_obj::<SgFilterMaintenance>(spec)?;
        Ok(filter.boxed())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterMaintenance {
    is_enabled: bool,
    title: Option<String>,
    msg: Option<String>,
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

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext, _matched_match_inst: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        if self.is_enabled {
            ctx.set_action(SgRouteFilterRequestAction::Response);
            let default_content_type = HeaderValue::from_static("text/html");
            let content_type = ctx.get_req_headers().get(header::CONTENT_TYPE).unwrap_or(&default_content_type).to_str().unwrap_or("");
            match content_type {
                "text/html" => {
                    let title = self.title.clone().unwrap_or("System Maintenance".to_string());
                    let msg = self.msg.clone().map(|x| x.replace("/n", "<br>")).unwrap_or("We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly.<br> Thank you for your patience, understanding, and support.".to_string());
                    ctx.set_resp_body(
                        format!(
                            r##"<!DOCTYPE html>
                    <html>
                    <head>
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
                        )
                        .into_bytes(),
                    )?;
                }
                "application/json" => {
                    let msg = self.msg.clone().unwrap_or("We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly.Thank you for your patience, understanding, and support.".to_string());
                    return Err(TardisError::forbidden(&msg, ""));
                }
                _ => {
                    ctx.set_resp_body("<h1>Maintenance</h1>".to_string().into_bytes())?;
                }
            }
        }
        Ok((true, ctx))
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext, _: Option<&SgHttpRouteMatchInst>) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}
