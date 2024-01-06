use std::{env, time::Duration, vec};

use serde_json::{json, Value};
use spacegate_kernel::config::{
    gateway_dto::{SgGateway, SgListener, SgProtocol, SgTlsConfig, SgTlsMode},
    http_route_dto::{SgBackendRef, SgHttpRoute, SgHttpRouteRule},
};
use spacegate_tower::BoxError;
use tardis::{
    basic::{
        tracing::{TardisTracingInitializer},
    },
    config::config_dto::WebClientModuleConfig,
    tokio::{self, time::sleep},
    web::web_client::{TardisHttpResponse, TardisWebClient},
};

const TLS_RSA_KEY: &str = r#"
-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAqVYYdfxTT9qr1np22UoIWq4v1E4cHncp35xxu4HNyZsoJBHR
K1gTvwh8x4LMe24lROW/LGWDRAyhaI8qDxxlitm0DPxU8p4iQoDQi3Z+oVKqsSwJ
pd3MRlu+4QFrveExwxgdahXvnhYgFJw5qG/IDWbQM0+ism/yRiXaxFNMI/kXe8FG
+JKSyJzR/yXPqM9ootgIzWxjmV50c+4eyr97DvbwAQcmHi3Ao96p4XoxzKlYWwE9
TA+s0NvmCgYxOdjLEClP8YVKbvSpFMi4dHMZId86xYioeFbr7XPp+2njr9oyZjpd
Xa9Fy5UhwZZqCqh+nQk0m3XUC5pSu3ZrPLxNNQIDAQABAoIBAFKtZJgGsK6md4vq
kyiYSufrcBLaaEQ/rkQtYCJKyC0NAlZKFLRy9oEpJbNLm4cQSkYPXn3Qunx5Jj2k
2MYz+SgIDy7f7KHgr52Ew020dzNQ52JFvBgt6NTZaqL1TKOS1fcJSSNIvouTBerK
NCSXHzfb4P+MfEVe/w1c4ilE+kH9SzdEo2jK/sRbzHIY8TX0JbmQ4SCLLayr22YG
usIxtIYcWt3MMP/G2luRnYzzBCje5MXdpAhlHLi4TB6x4h5PmBKYc57uOVNngKLd
YyrQKcszW4Nx5v0a4HG3A5EtUXNCco1+5asXOg2lYphQYVh2R+1wgu5WiDjDVu+6
EYgjFSkCgYEA0NBk6FDoxE/4L/4iJ4zIhu9BptN8Je/uS5c6wRejNC/VqQyw7SHb
hRFNrXPvq5Y+2bI/DxtdzZLKAMXOMjDjj0XEgfOIn2aveOo3uE7zf1i+njxwQhPu
uSYA9AlBZiKGr2PCYSDPnViHOspVJjxRuAgyWM1Qf+CTC0D95aj0oz8CgYEAz5n4
Cb3/WfUHxMJLljJ7PlVmlQpF5Hk3AOR9+vtqTtdxRjuxW6DH2uAHBDdC3OgppUN4
CFj55kzc2HUuiHtmPtx8mK6G+otT7Lww+nLSFL4PvZ6CYxqcio5MPnoYd+pCxrXY
JFo2W7e4FkBOxb5PF5So5plg+d0z/QiA7aFP1osCgYEAtgi1rwC5qkm8prn4tFm6
hkcVCIXc+IWNS0Bu693bXKdGr7RsmIynff1zpf4ntYGpEMaeymClCY0ppDrMYlzU
RBYiFNdlBvDRj6s/H+FTzHRk2DT/99rAhY9nzVY0OQFoQIXK8jlURGrkmI/CYy66
XqBmo5t4zcHM7kaeEBOWEKkCgYAYnO6VaRtPNQfYwhhoFFAcUc+5t+AVeHGW/4AY
M5qlAlIBu64JaQSI5KqwS0T4H+ZgG6Gti68FKPO+DhaYQ9kZdtam23pRVhd7J8y+
xMI3h1kiaBqZWVxZ6QkNFzizbui/2mtn0/JB6YQ/zxwHwcpqx0tHG8Qtm5ZAV7PB
eLCYhQKBgQDALJxU/6hMTdytEU5CLOBSMby45YD/RrfQrl2gl/vA0etPrto4RkVq
UrkDO/9W4mZORClN3knxEFSTlYi8YOboxdlynpFfhcs82wFChs+Ydp1eEsVHAqtu
T+uzn0sroycBiBfVB949LExnzGDFUkhG0i2c2InarQYLTsIyHCIDEA==
-----END RSA PRIVATE KEY-----
"#;

const TLS_CERT: &str = r#"
-----BEGIN CERTIFICATE-----
MIIEADCCAmigAwIBAgICAcgwDQYJKoZIhvcNAQELBQAwLDEqMCgGA1UEAwwhcG9u
eXRvd24gUlNBIGxldmVsIDIgaW50ZXJtZWRpYXRlMB4XDTE2MDgxMzE2MDcwNFoX
DTIyMDIwMzE2MDcwNFowGTEXMBUGA1UEAwwOdGVzdHNlcnZlci5jb20wggEiMA0G
CSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCpVhh1/FNP2qvWenbZSghari/UThwe
dynfnHG7gc3JmygkEdErWBO/CHzHgsx7biVE5b8sZYNEDKFojyoPHGWK2bQM/FTy
niJCgNCLdn6hUqqxLAml3cxGW77hAWu94THDGB1qFe+eFiAUnDmob8gNZtAzT6Ky
b/JGJdrEU0wj+Rd7wUb4kpLInNH/Jc+oz2ii2AjNbGOZXnRz7h7Kv3sO9vABByYe
LcCj3qnhejHMqVhbAT1MD6zQ2+YKBjE52MsQKU/xhUpu9KkUyLh0cxkh3zrFiKh4
Vuvtc+n7aeOv2jJmOl1dr0XLlSHBlmoKqH6dCTSbddQLmlK7dms8vE01AgMBAAGj
gb4wgbswDAYDVR0TAQH/BAIwADALBgNVHQ8EBAMCBsAwHQYDVR0OBBYEFMeUzGYV
bXwJNQVbY1+A8YXYZY8pMEIGA1UdIwQ7MDmAFJvEsUi7+D8vp8xcWvnEdVBGkpoW
oR6kHDAaMRgwFgYDVQQDDA9wb255dG93biBSU0EgQ0GCAXswOwYDVR0RBDQwMoIO
dGVzdHNlcnZlci5jb22CFXNlY29uZC50ZXN0c2VydmVyLmNvbYIJbG9jYWxob3N0
MA0GCSqGSIb3DQEBCwUAA4IBgQBsk5ivAaRAcNgjc7LEiWXFkMg703AqDDNx7kB1
RDgLalLvrjOfOp2jsDfST7N1tKLBSQ9bMw9X4Jve+j7XXRUthcwuoYTeeo+Cy0/T
1Q78ctoX74E2nB958zwmtRykGrgE/6JAJDwGcgpY9kBPycGxTlCN926uGxHsDwVs
98cL6ZXptMLTR6T2XP36dAJZuOICSqmCSbFR8knc/gjUO36rXTxhwci8iDbmEVaf
BHpgBXGU5+SQ+QM++v6bHGf4LNQC5NZ4e4xvGax8ioYu/BRsB/T3Lx+RlItz4zdU
XuxCNcm3nhQV2ZHquRdbSdoyIxV5kJXel4wCmOhWIq7A2OBKdu5fQzIAzzLi65EN
RPAKsKB4h7hGgvciZQ7dsMrlGw0DLdJ6UrFyiR5Io7dXYT/+JP91lP5xsl6Lhg9O
FgALt7GSYRm2cZdgi9pO9rRr83Br1VjQT1vHz6yoZMXSqc4A2zcN2a2ZVq//rHvc
FZygs8miAhWPzqnpmgTj1cPiU1M=
-----END CERTIFICATE-----
"#;

const TLS_PKCS8_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgRHa9BJGpo+H2vtsC
zj86Jw9nZC2iiRWkm1DaV/hxrMihRANCAAT3RsSUTWywEVmi5PaZHT+AdbTSbfjy
lZVZLARkWHAe7/O/rS1zYb995n93cQevTFxSZWRiC++QWfilebv+FiA5
-----END PRIVATE KEY-----"#;

const TLS_EC_KEY: &str = r#"-----BEGIN EC PRIVATE KEY-----
MHcCAQEEIER2vQSRqaPh9r7bAs4/OicPZ2QtookVpJtQ2lf4cazIoAoGCCqGSM49
AwEHoUQDQgAE90bElE1ssBFZouT2mR0/gHW00m348pWVWSwEZFhwHu/zv60tc2G/
feZ/d3EHr0xcUmVkYgvvkFn4pXm7/hYgOQ==
-----END EC PRIVATE KEY-----"#;

const TLS_EC_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIICKDCCAc2gAwIBAgIUdbUa6KZ5Hb0AnlSXkSsnK5t3Ok0wCgYIKoZIzj0EAwIw
aTELMAkGA1UEBhMCQ04xETAPBgNVBAgMCEhhbmdaaG91MRMwEQYDVQQKDAppZGVh
bHdvcmxkMRMwEQYDVQQLDAppZGVhbHdvcmxkMR0wGwYDVQQDDBR3d3cuaWRlYWx3
b3JsZC5ncm91cDAeFw0yMzA1MTYxMDAzMjlaFw0yNDA1MTUxMDAzMjlaMGkxCzAJ
BgNVBAYTAkNOMREwDwYDVQQIDAhIYW5nWmhvdTETMBEGA1UECgwKaWRlYWx3b3Js
ZDETMBEGA1UECwwKaWRlYWx3b3JsZDEdMBsGA1UEAwwUd3d3LmlkZWFsd29ybGQu
Z3JvdXAwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAAT3RsSUTWywEVmi5PaZHT+A
dbTSbfjylZVZLARkWHAe7/O/rS1zYb995n93cQevTFxSZWRiC++QWfilebv+FiA5
o1MwUTAdBgNVHQ4EFgQUauvZuVgb2eFu0FpYamvIp3ysM2gwHwYDVR0jBBgwFoAU
auvZuVgb2eFu0FpYamvIp3ysM2gwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQD
AgNJADBGAiEA9FhsNuvAJaEFclqqY8CZPYzpsziyn1CILpjIrt8U8cACIQDeFQvs
W0X+/YToWPeWivw3Kbo05oCob0NUi3fXtiTHng==
-----END CERTIFICATE-----"#;

#[tokio::test]
async fn test_https() -> Result<(), BoxError> {
    env::set_var("RUST_LOG", "info,spacegate_kernel=trace,spacegate_tower=trace,tower_service=trace,rust_tls=trace");
    let _tracing = TardisTracingInitializer::default().with_fmt_layer().with_env_layer().init();
    spacegate_kernel::do_startup(
        SgGateway {
            name: "test_gw".to_string(),
            listeners: vec![
                SgListener {
                    port: 8888,
                    protocol: SgProtocol::Https,
                    tls: Some(SgTlsConfig {
                        mode: SgTlsMode::Terminate,
                        key: TLS_RSA_KEY.to_string(),
                        cert: TLS_CERT.to_string(),
                    }),
                    ..Default::default()
                },
                SgListener {
                    port: 8889,
                    protocol: SgProtocol::Https,
                    tls: Some(SgTlsConfig {
                        mode: SgTlsMode::Terminate,
                        key: TLS_PKCS8_KEY.to_string(),
                        cert: TLS_EC_CERT.to_string(),
                    }),
                    ..Default::default()
                },
                SgListener {
                    port: 8890,
                    protocol: SgProtocol::Https,
                    tls: Some(SgTlsConfig {
                        mode: SgTlsMode::Terminate,
                        key: TLS_EC_KEY.to_string(),
                        cert: TLS_EC_CERT.to_string(),
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        vec![SgHttpRoute {
            hostnames: Some(vec!["localhost".to_string()]),
            gateway_name: "test_gw".to_string(),
            rules: Some(vec![SgHttpRouteRule {
                backends: Some(vec![SgBackendRef {
                    name_or_host: "postman-echo.com".to_string(),
                    port: 443,
                    protocol: Some(SgProtocol::Https),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }],
    )
    .await?;
    sleep(Duration::from_millis(500)).await;
    let client = TardisWebClient::init(&WebClientModuleConfig {
        connect_timeout_sec: 100,
        ..Default::default()
    })?;
    let resp: TardisHttpResponse<Value> = client
        .post(
            "https://localhost:8888/post?dd",
            &json!({
                "name":"星航",
                "age":6
            }),
            None,
        )
        .await?;
    assert!(resp.body.unwrap().get("data").unwrap().to_string().contains("星航"));

    let client = TardisWebClient::init(&WebClientModuleConfig {
        connect_timeout_sec: 100,
        ..Default::default()
    })?;
    let resp: TardisHttpResponse<Value> = client
        .post(
            "https://localhost:8889/post?dd",
            &json!({
                "name":"星航",
                "age":6
            }),
            None,
        )
        .await?;
    assert!(resp.body.unwrap().get("data").unwrap().to_string().contains("星航"));

    let client = TardisWebClient::init(&WebClientModuleConfig {
        connect_timeout_sec: 100,
        ..Default::default()
    })?;
    let resp: TardisHttpResponse<Value> = client
        .post(
            "https://localhost:8890/post?dd",
            &json!({
                "name":"星航",
                "age":6
            }),
            None,
        )
        .await?;
    assert!(resp.body.unwrap().get("data").unwrap().to_string().contains("星航"));
    Ok(())
}
