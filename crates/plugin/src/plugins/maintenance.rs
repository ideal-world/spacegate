use hyper::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
use hyper::{Request, Response, StatusCode};
use ipnet::IpNet;
use spacegate_kernel::extension::OriginalIpAddr;
use spacegate_kernel::helper_layers::function::Inner;
use spacegate_kernel::SgBody;
use spacegate_kernel::{BoxError, SgRequestExt};
use std::net::IpAddr;
use std::ops::Range;

use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::{Plugin, PluginError};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct MaintenancePluginConfig {
    /// enable time range
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    /// exclude ip range
    exclude_ip_range: Option<Vec<String>>,
    /// maintenance page title
    title: String,
    /// maintenance message
    msg: String,
    /// return code 307 to redirect to a specified path
    redirect: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MaintenancePlugin {
    enabled_time_range: Option<Vec<Range<NaiveTime>>>,
    title: String,
    msg: String,
    // redirect path and query
    redirect: Option<String>,
    exclude_ip_range: Option<Vec<IpNet>>,
}

impl MaintenancePlugin {
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

impl Default for MaintenancePluginConfig {
    fn default() -> Self {
        Self {
            enabled_time_range: None,
            exclude_ip_range: None,
            redirect: None,
            title: "System Maintenance".to_string(),
            msg: "We apologize for the inconvenience, but we are currently performing system maintenance. We will be back to normal shortly./n Thank you for your patience, understanding, and support.".to_string(),
        }
    }
}

impl Plugin for MaintenancePlugin {
    const CODE: &'static str = "maintenance";
    fn meta() -> spacegate_model::PluginMetaData {
        crate::plugin_meta!(
            description: "Maintenance page plugin."
        )
    }
    fn create(config: crate::PluginConfig) -> Result<Self, BoxError> {
        let plugin_config: MaintenancePluginConfig = serde_json::from_value(config.spec)?;

        let exclude_ip_range = plugin_config
            .exclude_ip_range
            .as_ref()
            .map(|exclude_ip_range| exclude_ip_range.iter().filter_map(|ip| ip.parse::<IpNet>().or(ip.parse::<IpAddr>().map(IpNet::from)).ok()).collect::<Vec<_>>());
        let plugin = MaintenancePlugin {
            enabled_time_range: plugin_config.enabled_time_range.clone(),
            title: plugin_config.title.clone(),
            msg: plugin_config.msg.clone(),
            exclude_ip_range,
            redirect: plugin_config.redirect,
        };
        Ok(plugin)
    }
    async fn call(&self, req: Request<SgBody>, inner: Inner) -> Result<Response<SgBody>, BoxError> {
        let original_ip = req.extract::<OriginalIpAddr>();

        if self.check_by_now() && !self.check_ip(&original_ip) {
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

            let mut resp = match content_type {
                ContentType::Html => {
                    let title = self.title.clone();
                    let msg = self.msg.clone().replace('\n', "<br>");
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
                    </html>
                    "##
                    );
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .header(CONTENT_TYPE, HeaderValue::from_static("text/html"))
                        .body(SgBody::full(body))
                        .map_err(PluginError::internal_error::<MaintenancePlugin>)?
                }
                ContentType::Json => Response::builder()
                    .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(SgBody::full(format!("\"{msg}\"", msg = self.msg)))
                    .map_err(PluginError::internal_error::<MaintenancePlugin>)?,
                ContentType::Other => Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header(CONTENT_TYPE, HeaderValue::from_static("text/html"))
                    .body(SgBody::full(format!("<h1>{}</h1>", self.title)))
                    .map_err(PluginError::internal_error::<MaintenancePlugin>)?,
            };
            if let Some(ref redirect) = self.redirect {
                if req.uri().path() == redirect {
                    return Ok(inner.call(req).await);
                }
                resp.headers_mut().insert("Location", HeaderValue::from_str(redirect)?);
                *resp.status_mut() = StatusCode::TEMPORARY_REDIRECT;
            }
            Ok(resp)
        } else {
            Ok(inner.call(req).await)
        }
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        use crate::PluginSchemaExt;
        Some(Self::schema())
    }
}

// def_plugin!("maintenance", MaintenancePlugin, SgFilterMaintenanceConfig; #[cfg(feature = "schema")] schema;);
#[cfg(feature = "schema")]
crate::schema!(MaintenancePlugin, MaintenancePluginConfig);
#[cfg(test)]
mod test {

    // use tardis::chrono::{Duration, Local};
    // use tardis::serde_json;
    // use tardis::tokio;

    // #[tokio::test]
    // async fn test_config() -> Result<(), BoxError> {
    //     let now = Local::now();
    //     let duration = Duration::try_seconds(100).expect("invalid seconds");
    //     let end_time = now + duration;
    //     let plugin = MaintenancePlugin::create(PluginConfig {
    //         spec: json!({
    //             "enabled_time_range": [
    //             {
    //                 "start": "10:00:00",
    //                 "end": "14:30:00"
    //             },
    //             {
    //                 "start": now.format("%H:%M:%S").to_string() ,
    //                 "end": end_time.format("%H:%M:%S").to_string()
    //             }
    //             ],
    //             "exclude_ip_range": [
    //                 "192.168.1.0/24",
    //                 "10.0.0.0/16",
    //                 "172.30.30.30"
    //             ]
    //         }),
    //         id: PluginInstanceId::new("maintenance", PluginInstanceName::named("test")),
    //     })
    //     .expect("invalid config");
    //     let inner = Inner::new(get_echo_service());

    //     let req = Request::builder()
    //         .method(Method::POST)
    //         .uri("http://sg.idealworld.group")
    //         .version(Version::HTTP_11)
    //         .extension(PeerAddr("192.168.1.123:10000".parse().expect("invalid addr")))
    //         .body(SgBody::empty())
    //         .expect("invalid request");
    //     let resp = plugin.call(req, inner.clone()).await.unwrap();
    //     assert_eq!(resp.status(), StatusCode::OK);

    //     let req = Request::builder()
    //         .method(Method::POST)
    //         .uri("http://sg.idealworld.group")
    //         .version(Version::HTTP_11)
    //         .extension(PeerAddr("192.168.2.123:10000".parse().expect("invalid addr")))
    //         .body(SgBody::empty())
    //         .expect("invalid request");
    //     let resp = plugin.call(req, inner.clone()).await.unwrap();
    //     assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    //     let req = Request::builder()
    //         .method(Method::POST)
    //         .uri("http://sg.idealworld.group")
    //         .version(Version::HTTP_11)
    //         .extension(PeerAddr("172.30.30.30:10000".parse().expect("invalid addr")))
    //         .body(SgBody::empty())
    //         .expect("invalid request");
    //     let resp = plugin.call(req, inner.clone()).await.unwrap();
    //     assert_eq!(resp.status(), StatusCode::OK);
    //     Ok(())
    // }
}
