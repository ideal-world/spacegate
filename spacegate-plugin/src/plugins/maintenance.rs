use hyper::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use hyper::{Request, Response, StatusCode};
use ipnet::IpNet;
use spacegate_tower::extension::PeerAddr;
use spacegate_tower::helper_layers::filter::{Filter, FilterRequestLayer};
use spacegate_tower::{SgBody, SgBoxLayer, SgResponseExt};
use std::net::IpAddr;
use std::ops::Range;
use tower::BoxError;

// use crate::def_filter;
// use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tardis::chrono::{Local, NaiveTime};

// use crate::plugins::context::SgRouteFilterRequestAction;

use crate::{def_plugin, MakeSgLayer};

// def_filter!("maintenance", SgMaintenanceFilterPlugin, SgFilterMaintenanceConfig);

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SgFilterMaintenanceConfig {
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    exclude_ip_range: Option<Vec<String>>,
    title: String,
    msg: String,
}

#[derive(Debug, Clone)]
pub struct SgFilterMaintenance {
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    title: String,
    msg: String,
    exclude_ip_range: Option<Vec<IpNet>>,
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
        if let Some(ips) = &self.exclude_ip_range {
            ips.iter().any(|allow_ip| allow_ip.contains(ip))
        } else {
            false
        }
    }
}

impl Default for SgFilterMaintenanceConfig {
    fn default() -> Self {
        Self {
            enabled_time_range: None,
            exclude_ip_range: None,
            title: "System Maintenance".to_string(),
            msg: "We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly./n Thank you for your patience, understanding, and support.".to_string(),
        }
    }
}

impl Filter for SgFilterMaintenance {
    fn filter(&self, req: Request<SgBody>) -> Result<hyper::Request<SgBody>, Response<SgBody>> {
        let Some(peer_id) = req.extensions().get::<PeerAddr>() else {
            return Err(Response::with_code_message(
                StatusCode::NOT_IMPLEMENTED,
                "missing peer addr info, it's an implemention error and please contact the maintener.",
            ));
        };

        if self.check_by_now() && !self.check_ip(&peer_id.0.ip()) {
            // let content_types = req.headers().get(CONTENT_TYPE).map(|content_type| content_type.to_str().unwrap_or("").split(','));
            let accept_types = req.headers().get(ACCEPT).map(|accept| accept.to_str().unwrap_or("").split(','));

            enum ContentType {
                Html,
                Json,
                Other,
            }
            let content_type = if let Some(mut accept_types) = accept_types {
                loop {
                    match accept_types.next() {
                        Some("text/html") => break ContentType::Html,
                        Some("application/json") => break ContentType::Json,
                        Some(_) => continue,
                        None => break ContentType::Other,
                    }
                }
            } else {
                ContentType::Other
            };

            let resp = match content_type {
                ContentType::Html => {
                    let title = self.title.clone();
                    let msg = self.msg.clone().replace("/n", "<br>");
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
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .header(CONTENT_TYPE, HeaderValue::from_static("text/html"))
                        .body(SgBody::full(body))
                        .map_err(Response::internal_error)?
                }
                ContentType::Json => Response::builder()
                    .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(SgBody::full(format!("\"{msg}\"", msg = self.msg)))
                    .map_err(Response::internal_error)?,
                ContentType::Other => Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header(CONTENT_TYPE, HeaderValue::from_static("text/html"))
                    .body(SgBody::full(format!("<h1>{}</h1>", self.title)))
                    .map_err(Response::internal_error)?,
            };
            Err(resp)
        } else {
            Ok(req)
        }
    }
}

impl MakeSgLayer for SgFilterMaintenanceConfig {
    fn make_layer(&self) -> Result<SgBoxLayer, BoxError> {
        let exclude_ip_range = self
            .exclude_ip_range
            .as_ref()
            .map(|exclude_ip_range| exclude_ip_range.iter().filter_map(|ip| ip.parse::<IpNet>().or(ip.parse::<IpAddr>().map(IpNet::from)).ok()).collect::<Vec<_>>());
        let filter = SgFilterMaintenance {
            enabled_time_range: self.enabled_time_range.clone(),
            title: self.title.clone(),
            msg: self.msg.clone(),
            exclude_ip_range,
        };
        Ok(SgBoxLayer::new(FilterRequestLayer::new(filter)))
    }
}

def_plugin!("maintenance", MaintenancePlugin, SgFilterMaintenanceConfig);

#[cfg(test)]
mod test {

    use hyper::StatusCode;
    use hyper::{Method, Request, Version};
    use serde_json::json;
    use spacegate_tower::extension::PeerAddr;
    use spacegate_tower::service::get_echo_service;
    use spacegate_tower::SgBody;
    use tardis::chrono::{Duration, Local};
    use tardis::serde_json;
    use tardis::tokio;
    use tower::{BoxError, ServiceExt};
    use tower_layer::Layer;
    use tower_service::Service;

    #[tokio::test]
    async fn test_config() -> Result<(), BoxError> {
        let now = Local::now();
        let duration = Duration::seconds(100);
        let end_time = now + duration;
        let repo = crate::SgPluginRepository::global()
            .create_layer(
                crate::plugins::maintenance::CODE,
                json!({
                  "enabled_time_range": [
                    {
                      "start": "10:00:00",
                      "end": "14:30:00"
                    },
                    {
                      "start": now.format("%H:%M:%S").to_string() ,
                      "end": end_time.format("%H:%M:%S").to_string()
                    }
                  ],
                  "exclude_ip_range": [
                       "192.168.1.0/24",
                       "10.0.0.0/16",
                       "172.30.30.30"
                  ]
                }
                ),
            )
            .expect("unable to create plugin");
        let mut serivce = repo.layer(get_echo_service());

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://sg.idealworld.group")
            .version(Version::HTTP_11)
            .extension(PeerAddr("192.168.1.123:10000".parse().expect("invalid addr")))
            .body(SgBody::empty())
            .expect("invalid request");
        let resp = serivce.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://sg.idealworld.group")
            .version(Version::HTTP_11)
            .extension(PeerAddr("192.168.2.123:10000".parse().expect("invalid addr")))
            .body(SgBody::empty())
            .expect("invalid request");
        let resp = serivce.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        let req = Request::builder()
            .method(Method::POST)
            .uri("http://sg.idealworld.group")
            .version(Version::HTTP_11)
            .extension(PeerAddr("172.30.30.30:10000".parse().expect("invalid addr")))
            .body(SgBody::empty())
            .expect("invalid request");
        let resp = serivce.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        Ok(())
    }
}
