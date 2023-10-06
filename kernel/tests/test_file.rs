// use std::{env, mem};
// use std::time::Duration;
// use http::{header, HeaderMap, Method};
// use hyper::{Body, Client};
// use hyper_rustls::ConfigBuilderExt;
// use tardis::tokio;
// use tardis::tokio::time::sleep;
// use spacegate_kernel::config::gateway_dto::{SgGateway, SgListener, SgProtocol};
// use spacegate_kernel::config::http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule};
// use spacegate_kernel::functions::http_client;
//
// #[tokio::test]
// async fn test_https() {
//     env::set_var("RUST_LOG", "info,spacegate_kernel=trace");
//     tracing_subscriber::fmt::init();
//     spacegate_kernel::do_startup(
//         SgGateway {
//             name: "test_gw".to_string(),
//             listeners: vec![
//                 SgListener {
//                     port: 8888,
//                     ..Default::default()
//                 },],
//             ..Default::default()
//         },
//         vec![SgHttpRoute {
//             gateway_name: "test_gw".to_string(),
//             rules: Some(vec![SgHttpRouteRule {
//                 backends: Some(vec![SgBackendRef {
//                     name_or_host: "postman-echo.com".to_string(),
//                     port: 443,
//                     protocol: Some(SgProtocol::Https),
//                     ..Default::default()
//                 }]),
//                 ..Default::default()
//             }]),
//             ..Default::default()
//         }],
//     )
//         .await.unwrap();
//     sleep(Duration::from_millis(500)).await;
//
//     let https = hyper_rustls::HttpsConnectorBuilder::new().with_tls_config(rustls::ClientConfig::builder().with_safe_defaults().with_native_roots().with_no_client_auth()).https_or_http().enable_http1().build();
//     let client = Client::builder().build(https);
//
//     let body=Body::empty();
//     let mut headers=HeaderMap::new();
//     headers.insert(header::CONTENT_TYPE,"multipart/form-data; boundary=--------------------------734948368826402972978072".parse().unwrap());
//     let resp=http_client::raw_request(Some(&client),Method::POST,"http://localhost:8888/post",body,&headers,None).await.unwrap();
//
//     let mut resp_body =resp.body();
//     let mut swap_body=resp_body
//     mem::swap(&mut resp_body);
//     let bytes = hyper::body::to_bytes().await.unwrap();
//
//     println!("{:?}",resp_body);
//
// }
