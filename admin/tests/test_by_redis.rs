use std::env;

use kernel_common::client::cache_client::{CONF_GATEWAY_KEY, CONF_HTTP_ROUTE_KEY};
use kernel_common::gatewayapi_support_filter::SgFilterRewrite;
use kernel_common::inner_model::gateway::{SgParameters, SgProtocol, SgTls, SgTlsMode};
use kernel_common::inner_model::http_route::{SgHttpHeaderMatch, SgHttpHeaderMatchType, SgHttpPathMatch, SgHttpPathMatchType, SgHttpRouteMatch};
use kernel_common::inner_model::plugin_filter::{SgHttpPathModifier, SgHttpPathModifierType};
use spacegate_admin::config::SpacegateAdminConfig;
use spacegate_admin::model::vo::backend_vo::SgBackendRefVo;
use spacegate_admin::model::vo::gateway_vo::{SgGatewayVo, SgListenerVo, SgTlsConfigVo};
use spacegate_admin::model::vo::http_route_vo::{SgHttpRouteRuleVo, SgHttpRouteVo};
use spacegate_admin::model::vo::plugin_vo::SgFilterVo;
use tardis::log;
use tardis::url::Url;
use tardis::{
    basic::result::TardisResult,
    cache::cache_client::TardisCacheClient,
    config::config_dto::{CacheConfig, CacheModuleConfig, FrameworkConfig, TardisConfig, WebServerCommonConfig, WebServerConfig, WebServerModuleConfig},
    test::test_container::TardisTestContainer,
    testcontainers, tokio, TardisFuns,
};

mod test_http_client;
use test_http_client::TestHttpClient;

#[tokio::test]
async fn test_api_by_redis() -> TardisResult<()> {
    env::set_var("RUST_LOG", "info,tardis=trace,spacegate_admin=trace");
    tracing_subscriber::fmt::init();

    let admin_port = 9081;
    let http_client = TestHttpClient::new(&format!("http://127.0.0.1:{admin_port}")).await?;

    let docker = testcontainers::clients::Cli::default();
    let redis_container = TardisTestContainer::redis_custom(&docker);
    let port = redis_container.get_host_port_ipv4(6379);
    let cache_url = format!("redis://127.0.0.1:{port}/0",);

    // Start admin
    TardisFuns::init_conf(
        TardisConfig::builder()
            .fw(FrameworkConfig::builder()
                .cache(CacheConfig::builder().default(CacheModuleConfig::builder().url(Url::parse(&cache_url).unwrap()).build()).build())
                .web_server(
                    WebServerConfig::builder()
                        .common(WebServerCommonConfig::builder().port(admin_port).build())
                        .default(WebServerModuleConfig::builder().build())
                        .modules([("admin".to_string(), WebServerModuleConfig::builder().build())])
                        .build(),
                )
                .build())
            .cs([("admin".to_string(), TardisFuns::json.obj_to_json(&SpacegateAdminConfig::default()).unwrap())])
            .build(),
    )
    .await?;
    let web_server = TardisFuns::web_server();
    spacegate_admin::initializer::init(&web_server).await?;
    web_server.start().await?;

    let cache_client = TardisCacheClient::init(&CacheModuleConfig {
        url: cache_url.parse().expect("invalid url"),
    })
    .await?;

    test_gateway(&http_client, &cache_client, &cache_url).await?;
    test_httproute(&http_client, &cache_client).await?;
    test_sgfilter(&http_client, &cache_client).await?;

    Ok(())
}

async fn test_gateway(http_client: &TestHttpClient, cache_client: &TardisCacheClient, cache_url: &str) -> TardisResult<()> {
    log::info!("[Admin.Test] Start Test Gateway");
    let gateway_api_url = "/admin/gateway";
    let tls_api_url = "/admin/tls";

    let test_gateway_1 = SgGatewayVo {
        name: "test".to_string(),
        parameters: SgParameters {
            redis_url: Some(cache_url.to_string()),
            log_level: Some("info".to_string()),
            lang: None,
            ignore_tls_verification: Some(true),
        },
        listeners: vec![SgListenerVo {
            name: "listener-1".to_string(),
            ip: None,
            port: 18080,
            protocol: SgProtocol::default(),
            tls: None,
            hostname: None,
            ..Default::default()
        }],
        filters: Vec::new(),
        ..Default::default()
    };
    //Add gateway
    let add_result: SgGatewayVo = http_client.post(gateway_api_url, &test_gateway_1, None).await.unwrap();

    assert_eq!(add_result, test_gateway_1);

    let mut result: Vec<SgGatewayVo> = http_client.get(gateway_api_url, None).await?;

    assert_eq!(result.len(), 1);
    assert_eq!(result.remove(0), test_gateway_1);

    //Modify gateway

    // 1. add Secret
    let test_tls_id = "test_tls";
    let test_tls = SgTls {
        name: test_tls_id.to_string(),
        key: String::new(),
        cert: String::new(),
    };
    let add_result: SgTls = http_client.post(tls_api_url, &test_tls, None).await?;
    assert_eq!(add_result, test_tls);

    let test_gateway_2 = SgGatewayVo {
        parameters: SgParameters {
            redis_url: None,
            log_level: Some("debug".to_string()),
            lang: None,
            ignore_tls_verification: Some(false),
        },
        listeners: vec![SgListenerVo {
            name: "listener-1".to_string(),
            ip: Some("127.0.0.1".to_string()),
            port: 18081,
            protocol: SgProtocol::default(),
            tls: Some(SgTlsConfigVo {
                name: test_tls_id.to_string(),
                mode: SgTlsMode::default(),
            }),
            hostname: None,
            ..Default::default()
        }],
        ..test_gateway_1
    };
    let modify_result: SgGatewayVo = http_client.put(gateway_api_url, &test_gateway_2, None).await.unwrap();
    assert_eq!(modify_result, test_gateway_2);

    let get_redis = cache_client.hget(CONF_GATEWAY_KEY, "test").await?.unwrap();

    assert_eq!(
        get_redis,
        r#"{"name":"test","parameters":{"redis_url":null,"log_level":"debug","lang":null,"ignore_tls_verification":false},"listeners":[{"name":"listener-1","ip":"127.0.0.1","port":18081,"protocol":"Http","tls":{"mode":"Passthrough","tls":{"name":"test_tls","key":"","cert":""}},"hostname":null}],"filters":null}"#
    );

    // 2. modify secret

    let _: SgTls = http_client
        .put(
            tls_api_url,
            &SgTls {
                key: "test_key".to_string(),
                cert: "test_cert".to_string(),
                ..test_tls
            },
            None,
        )
        .await?;

    let get_redis = cache_client.hget(CONF_GATEWAY_KEY, "test").await?.unwrap();

    assert_eq!(
        get_redis,
        r#"{"name":"test","parameters":{"redis_url":null,"log_level":"debug","lang":null,"ignore_tls_verification":false},"listeners":[{"name":"listener-1","ip":"127.0.0.1","port":18081,"protocol":"Http","tls":{"mode":"Passthrough","tls":{"name":"test_tls","key":"test_key","cert":"test_cert"}},"hostname":null}],"filters":null}"#
    );
    //3. delete tls
    assert!(http_client.delete(&format!("{tls_api_url}/{test_tls_id}",), None).await.is_err());

    //Delete gateway
    http_client.delete(&format!("{gateway_api_url}/test"), None).await?;

    result = http_client.get(gateway_api_url, None).await?;
    assert_eq!(result.len(), 0);

    assert!(cache_client.hget(CONF_GATEWAY_KEY, "test").await?.is_none());

    //delete tls
    assert!(http_client.delete(&format!("{tls_api_url}/{test_tls_id}",), None).await.is_ok());

    Ok(())
}

async fn test_httproute(http_client: &TestHttpClient, cache_client: &TardisCacheClient) -> TardisResult<()> {
    log::info!("[Admin.Test] Start Test HttpRoute");
    let gateway_api_url = "/admin/gateway";
    let httproute_api_url = "/admin/httproute";
    let backend_api_url = "/admin/backend";

    //Add route
    let _: SgGatewayVo = http_client
        .post(
            gateway_api_url,
            &SgGatewayVo {
                name: "test_gw".to_string(),
                parameters: SgParameters::default(),
                listeners: vec![SgListenerVo {
                    name: "listener-1".to_string(),
                    port: 18080,
                    protocol: SgProtocol::default(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

    // 1.add backend
    let test_backend_1_id = "test_backend_1".to_string();
    let test_backend_1 = SgBackendRefVo {
        id: test_backend_1_id.clone(),
        name_or_host: "test1.com".to_string(),
        port: 80,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let add_result: SgBackendRefVo = http_client.post(backend_api_url, &test_backend_1, None).await.unwrap();
    assert_eq!(add_result, test_backend_1);

    let test_backend_2_id = "test_backend_2".to_string();
    let test_backend_2 = SgBackendRefVo {
        id: test_backend_2_id.clone(),
        name_or_host: "test2.com".to_string(),
        port: 8080,
        timeout_ms: Some(1500),
        weight: Some(10),
        protocol: Some(SgProtocol::Https),
        ..Default::default()
    };
    let add_result: SgBackendRefVo = http_client.post(backend_api_url, &test_backend_2, None).await.unwrap();
    assert_eq!(add_result, test_backend_2);

    let test_http_1 = SgHttpRouteVo {
        name: "test".to_string(),
        gateway_name: "test_gw".to_string(),
        priority: 60,
        hostnames: Some(vec!["test.com".to_string()]),
        rules: vec![SgHttpRouteRuleVo {
            matches: Some(vec![SgHttpRouteMatch {
                path: Some(SgHttpPathMatch {
                    kind: SgHttpPathMatchType::default(),
                    value: "/".to_string(),
                }),
                header: None,
                query: None,
                method: None,
            }]),
            backends: vec![test_backend_1_id.clone()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let add_result: SgHttpRouteVo = http_client.post(httproute_api_url, &test_http_1, None).await.unwrap();
    assert_eq!(add_result, test_http_1);

    //Modify route
    let modify_test_http_1 = SgHttpRouteVo {
        priority: 180,
        hostnames: Some(vec!["test123.com".to_string()]),
        rules: vec![SgHttpRouteRuleVo {
            matches: Some(vec![SgHttpRouteMatch {
                path: Some(SgHttpPathMatch {
                    kind: SgHttpPathMatchType::Exact,
                    value: "/iam".to_string(),
                }),
                header: Some(vec![SgHttpHeaderMatch {
                    kind: SgHttpHeaderMatchType::Exact,
                    name: "X-Test".to_string(),
                    value: "test".to_string(),
                }]),
                query: None,
                method: None,
            }]),
            backends: vec![test_backend_2_id.clone()],
            timeout_ms: Some(10000),
            ..Default::default()
        }],
        ..test_http_1
    };
    let modify_result: SgHttpRouteVo = http_client.put(httproute_api_url, &modify_test_http_1, None).await?;
    assert_eq!(modify_result, modify_test_http_1);

    let http_route_configs = cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{}", "test_gw")).await?;

    assert_eq!(http_route_configs.len(), 1);
    assert_eq!(
        http_route_configs.get(0).unwrap(),
        r#"{"name":"test","gateway_name":"test_gw","priority":180,"hostnames":["test123.com"],"filters":null,"rules":[{"matches":[{"path":{"kind":"Exact","value":"/iam"},"header":[{"kind":"Exact","name":"X-Test","value":"test"}],"query":null,"method":null}],"filters":null,"backends":[{"name_or_host":"test2.com","namespace":null,"port":8080,"timeout_ms":1500,"protocol":"Https","weight":10,"filters":null}],"timeout_ms":10000}]}"#
    );

    // 2. modify backend
    let _: SgBackendRefVo = http_client
        .put(
            backend_api_url,
            &SgBackendRefVo {
                port: 8081,
                timeout_ms: Some(2000),
                weight: Some(5),
                protocol: Some(SgProtocol::Http),
                ..test_backend_2
            },
            None,
        )
        .await
        .unwrap();

    let http_route_configs = cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{}", "test_gw")).await?;

    assert_eq!(http_route_configs.len(), 1);
    assert_eq!(
        http_route_configs.get(0).unwrap(),
        r#"{"name":"test","gateway_name":"test_gw","priority":180,"hostnames":["test123.com"],"filters":null,"rules":[{"matches":[{"path":{"kind":"Exact","value":"/iam"},"header":[{"kind":"Exact","name":"X-Test","value":"test"}],"query":null,"method":null}],"filters":null,"backends":[{"name_or_host":"test2.com","namespace":null,"port":8081,"timeout_ms":2000,"protocol":"Http","weight":5,"filters":null}],"timeout_ms":10000}]}"#
    );

    // 3. delete backend
    assert!(http_client.delete(&format!("{backend_api_url}/{}", test_backend_2_id), None).await.is_err());

    //Delete route
    assert!(http_client.delete(&format!("{httproute_api_url}/{}", modify_test_http_1.name.clone()), None).await.is_ok());

    let result: Vec<SgHttpRouteVo> = http_client.get(httproute_api_url, None).await?;
    assert_eq!(result.len(), 0);

    assert_eq!(cache_client.llen(&format!("{CONF_HTTP_ROUTE_KEY}{}", "test_gw")).await?, 0);

    Ok(())
}

async fn test_sgfilter(http_client: &TestHttpClient, cache_client: &TardisCacheClient) -> TardisResult<()> {
    log::info!("[Admin.Test] Start Test SgFilter");
    let gateway_api_url = "/admin/gateway";
    let httproute_api_url = "/admin/httproute";
    let plugin_api_url = "/admin/plugin";

    //Add Plugin
    let mut test_filter_1 = SgFilterVo {
        code: "rewrite".to_string(),
        enable: true,
        name: Some("rewrite-test".to_string()),
        spec: TardisFuns::json.obj_to_json(&SgFilterRewrite {
            hostname: Some("iam".to_string()),
            path: Some(SgHttpPathModifier {
                kind: SgHttpPathModifierType::ReplacePrefixMatch,
                value: "/iam".to_string(),
            }),
        })?,
        ..Default::default()
    };

    let mut test_filter_2 = SgFilterVo {
        code: "status".to_string(),
        enable: true,
        name: Some("status-test".to_string()),
        spec: serde_json::Value::Null,
        ..Default::default()
    };

    let add_result: SgFilterVo = http_client.post(plugin_api_url, &test_filter_1, None).await?;
    test_filter_1.id = add_result.id.clone();
    assert_eq!(add_result, test_filter_1);
    let plugin_id_1 = add_result.id;

    let add_result: SgFilterVo = http_client.post(plugin_api_url, &test_filter_2, None).await?;
    test_filter_2.id = add_result.id.clone();
    assert_eq!(add_result, test_filter_2);
    let plugin_id_2 = add_result.id;

    //1.add gateway
    let gateway_id = "test_gw_1";
    let _: SgGatewayVo = http_client
        .post(
            gateway_api_url,
            &SgGatewayVo {
                name: gateway_id.to_string(),
                filters: vec![plugin_id_2.to_string()],
                ..Default::default()
            },
            None,
        )
        .await?;

    let get_redis = cache_client.hget(CONF_GATEWAY_KEY, gateway_id).await?.unwrap();

    assert_eq!(
        get_redis,
        r#"{"name":"test_gw_1","parameters":{"redis_url":null,"log_level":null,"lang":null,"ignore_tls_verification":null},"listeners":[],"filters":[{"code":"status","enable":true,"name":"status-test","spec":null}]}"#
    );

    //2.add route
    let _: SgHttpRouteVo = http_client
        .post(
            httproute_api_url,
            &SgHttpRouteVo {
                name: "test_route".to_string(),
                gateway_name: gateway_id.to_string(),
                filters: vec![plugin_id_1.to_string()],
                ..Default::default()
            },
            None,
        )
        .await?;

    let http_route_configs = cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{gateway_id}")).await?;
    assert_eq!(http_route_configs.len(), 1);
    assert_eq!(
        http_route_configs.get(0).unwrap(),
        r#"{"name":"test_route","gateway_name":"test_gw_1","priority":0,"hostnames":null,"filters":[{"code":"rewrite","enable":true,"name":"rewrite-test","spec":{"hostname":"iam","path":{"kind":"replaceprefixmatch","value":"/iam"}}}],"rules":null}"#
    );

    // modify plugin
    let _: SgFilterVo = http_client
        .put(
            plugin_api_url,
            &SgFilterVo {
                spec: TardisFuns::json.obj_to_json(&SgFilterRewrite {
                    hostname: Some("iam".to_string()),
                    path: Some(SgHttpPathModifier {
                        kind: SgHttpPathModifierType::ReplaceFullPath,
                        value: "/auth".to_string(),
                    }),
                })?,
                ..test_filter_1
            },
            None,
        )
        .await?;

    let http_route_configs = cache_client.lrangeall(&format!("{CONF_HTTP_ROUTE_KEY}{gateway_id}")).await?;
    assert_eq!(http_route_configs.len(), 1);
    assert_eq!(
        http_route_configs.get(0).unwrap(),
        r#"{"name":"test_route","gateway_name":"test_gw_1","priority":0,"hostnames":null,"filters":[{"code":"rewrite","enable":true,"name":"rewrite-test","spec":{"hostname":"iam","path":{"kind":"replacefullpath","value":"/auth"}}}],"rules":null}"#
    );

    //delete plugin
    assert!(http_client.delete(&format!("{plugin_api_url}/{plugin_id_1}"), None).await.is_err());

    Ok(())
}
