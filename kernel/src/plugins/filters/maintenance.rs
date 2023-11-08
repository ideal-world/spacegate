use async_trait::async_trait;
use http::header;
use ipnet::IpNet;
use std::net::IpAddr;
use std::ops::Range;

use crate::def_filter;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tardis::basic::{error::TardisError, result::TardisResult};
use tardis::chrono::{Local, NaiveTime};

use crate::plugins::context::SgRouteFilterRequestAction;

use super::{SgPluginFilter, SgPluginFilterInitDto, SgRoutePluginContext};

def_filter!("maintenance", SgFilterMaintenanceDef, SgFilterMaintenance);

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterMaintenance {
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    exclude_ip_range: Option<Vec<String>>,
    title: String,
    msg: String,
    #[serde(skip)]
    exclude_ip_net: Option<Vec<IpNet>>,
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

    /// If the current time is within the set range, return true
    pub fn check_by_now(&self) -> bool {
        let local_time = Local::now().time();
        self.check_by_time(local_time)
    }

    /// If the parameter ip is within the setting range, return true
    pub fn check_ip(&self, ip: &IpAddr) -> bool {
        if let Some(ips) = &self.exclude_ip_net {
            ips.iter().any(|allow_ip| allow_ip.contains(ip))
        } else {
            false
        }
    }
}

impl Default for SgFilterMaintenance {
    fn default() -> Self {
        Self {
            enabled_time_range: None,
            exclude_ip_range: None,
            title: "System Maintenance".to_string(),
            msg: "We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly./n Thank you for your patience, understanding, and support.".to_string(),
            exclude_ip_net: None,
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
        if let Some(ip) = &self.exclude_ip_range {
            let ip_net = ip.iter().filter_map(|ip| ip.parse::<IpNet>().or(ip.parse::<IpAddr>().map(IpNet::from)).ok()).collect();
            self.exclude_ip_net = Some(ip_net);
        }
        Ok(())
    }

    async fn destroy(&self) -> TardisResult<()> {
        Ok(())
    }

    async fn req_filter(&self, _: &str, mut ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        if self.check_by_now() && !self.check_ip(&ctx.request.get_remote_addr().ip()) {
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
            Ok((false, ctx))
        } else {
            Ok((true, ctx))
        }
    }

    async fn resp_filter(&self, _: &str, ctx: SgRoutePluginContext) -> TardisResult<(bool, SgRoutePluginContext)> {
        Ok((true, ctx))
    }
}

#[cfg(test)]
mod test {
    use crate::plugins::context::SgRouteFilterRequestAction;
    use crate::plugins::context::SgRoutePluginContext;
    use crate::plugins::filters::maintenance::SgFilterMaintenanceDef;
    use crate::plugins::filters::{SgAttachedLevel, SgPluginFilterDef, SgPluginFilterInitDto};
    use http::{HeaderMap, Method, Uri, Version};
    use hyper::Body;
    use serde_json::json;
    use tardis::chrono::{Duration, Local};
    use tardis::tokio;

    #[tokio::test]
    async fn test_config() {
        let now = Local::now();
        let duration = Duration::seconds(100);
        let end_time = now + duration;
        let mut maintenance = SgFilterMaintenanceDef {}
            .inst(json!({
              "enabled_time_range": [
                {
                  "start": "10:00:00",
                  "end": "14:30:00"
                },
                {
                  "start":now.format("%H:%M:%S").to_string() ,
                  "end": end_time.format("%H:%M:%S").to_string()
                }
              ],
              "exclude_ip_range": [
                   "192.168.1.0/24",
                   "10.0.0.0/16",
                   "172.30.30.30"
              ]
            }
            ))
            .unwrap();

        maintenance
            .init(&SgPluginFilterInitDto {
                gateway_name: "".to_string(),
                gateway_parameters: Default::default(),
                http_route_rules: vec![],
                attached_level: SgAttachedLevel::Gateway,
            })
            .await
            .unwrap();

        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "192.168.1.123:10000".parse().unwrap(),
            String::new(),
            None,
            None,
        );
        assert_eq!(maintenance.req_filter("", ctx).await.unwrap().1.get_action(), &SgRouteFilterRequestAction::None);

        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "192.168.2.123:10000".parse().unwrap(),
            String::new(),
            None,
            None,
        );
        assert_eq!(maintenance.req_filter("", ctx).await.unwrap().1.get_action(), &SgRouteFilterRequestAction::Response);

        let ctx = SgRoutePluginContext::new_http(
            Method::POST,
            Uri::from_static("http://sg.idealworld.group"),
            Version::HTTP_11,
            HeaderMap::new(),
            Body::empty(),
            "172.30.30.30:10000".parse().unwrap(),
            String::new(),
            None,
            None,
        );
        assert_eq!(maintenance.req_filter("", ctx).await.unwrap().1.get_action(), &SgRouteFilterRequestAction::None);
    }
}
