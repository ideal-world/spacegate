use spacegate_model::{Config, McpSessionAffinity, McpTimeoutMode, SgMcpTransport, SgRoute};

#[test]
fn test_parse_config() {
    let file = include_str!("test_parse_config/config.toml");
    let parse_result = toml::from_str::<Config>(file);
    match parse_result {
        Ok(result) => {
            println!("{:#?}", result);
        }
        Err(e) => {
            eprintln!("{}", e);
            if let Some(span) = e.span() {
                let bytes = file.as_bytes();
                let span_str = std::str::from_utf8(&bytes[span]).unwrap();
                eprintln!("{}", span_str);
            }
            panic!();
        }
    }
}

#[test]
fn observability_defaults_to_disabled() {
    let config = Config::default();

    assert!(!config.observability.enabled);
    assert_eq!(config.observability.service_name, "spacegate");
    assert_eq!(config.observability.otlp_endpoint, "http://localhost:4317");
    assert!(!config.observability.traces.enabled);
    assert!(!config.observability.metrics.enabled);
    assert!(!config.observability.logs.enabled);
}

#[test]
fn observability_can_be_parsed_from_config() {
    let file = r#"
[observability]
enabled = true
service_name = "spacegate-test"
otlp_endpoint = "http://collector:4317"
protocol = "grpc"

[observability.traces]
enabled = true
sample_ratio = 0.5

[observability.metrics]
enabled = true
export_interval_ms = 10000

[observability.logs]
enabled = true
level = "warn"
"#;

    let config = toml::from_str::<Config>(file).expect("parse config");

    assert!(config.observability.enabled);
    assert_eq!(config.observability.service_name, "spacegate-test");
    assert_eq!(config.observability.otlp_endpoint, "http://collector:4317");
    assert_eq!(config.observability.protocol, spacegate_model::OtlpProtocol::Grpc);
    assert!(config.observability.traces.enabled);
    assert_eq!(config.observability.traces.sample_ratio, 0.5);
    assert!(config.observability.metrics.enabled);
    assert_eq!(config.observability.metrics.export_interval_ms, 10000);
    assert!(config.observability.logs.enabled);
    assert_eq!(config.observability.logs.level, "warn");
}

#[test]
fn local_otel_json_config_shape_can_be_parsed() {
    let file = r#"
{
  "api_port": 9876,
  "observability": {
    "enabled": true,
    "service_name": "spacegate-local-otel",
    "otlp_endpoint": "http://127.0.0.1:4317",
    "protocol": "grpc",
    "traces": {
      "enabled": true,
      "sample_ratio": 1.0
    },
    "metrics": {
      "enabled": true,
      "export_interval_ms": 5000
    },
    "logs": {
      "enabled": true,
      "level": "info"
    }
  },
  "gateways": {
    "local": {
      "gateway": {
        "name": "local",
        "parameters": {
          "enable_x_request_id": true
        },
        "listeners": [
          {
            "name": "http",
            "ip": "0.0.0.0",
            "port": 9000,
            "protocol": {
              "type": "http"
            }
          }
        ]
      },
      "routes": {
        "root": {
          "route_name": "root",
          "rules": [
            {
              "matches": [
                {
                  "path": {
                    "kind": "Prefix",
                    "value": "/"
                  }
                }
              ],
              "backends": [
                {
                  "host": {
                    "kind": "Host",
                    "host": "127.0.0.1"
                  },
                  "port": 18080,
                  "protocol": "http",
                  "weight": 1
                }
              ]
            }
          ]
        }
      }
    }
  }
}
"#;

    let config = serde_json::from_str::<Config>(file).expect("parse local otel json config");
    let gateway = config.gateways.get("local").expect("local gateway");

    assert_eq!(gateway.gateway.name, "local");
    assert_eq!(gateway.gateway.listeners.len(), 1);
    assert_eq!(gateway.gateway.listeners[0].port, 9000);
    assert!(gateway.routes.contains_key("root"));
}

#[test]
fn legacy_http_route_without_kind_is_wrapped_as_http_route() {
    let file = r#"
{
  "gateways": {
    "local": {
      "gateway": {
        "name": "local",
        "listeners": []
      },
      "routes": {
        "root": {
          "route_name": "root",
          "rules": []
        }
      }
    }
  }
}
"#;

    let config = serde_json::from_str::<Config>(file).expect("parse legacy http route");
    let route = config.gateways["local"].routes.get("root").expect("root route");

    match route {
        SgRoute::Http(route) => assert_eq!(route.route_name, "root"),
        SgRoute::Mcp(_) => panic!("legacy HTTP route should parse as SgRoute::Http"),
    }
}

#[test]
fn mcp_route_parses_with_streamable_defaults() {
    let file = r#"
{
  "gateways": {
    "local": {
      "gateway": {
        "name": "local",
        "listeners": []
      },
      "routes": {
        "mcp": {
          "kind": "MCPRoute",
          "route_name": "mcp",
          "path": "/mcp",
          "backends": [
            {
              "host": {
                "kind": "Host",
                "host": "127.0.0.1"
              },
              "port": 3001,
              "protocol": "http"
            }
          ]
        }
      }
    }
  }
}
"#;

    let config = serde_json::from_str::<Config>(file).expect("parse mcp route");
    let route = config.gateways["local"].routes.get("mcp").expect("mcp route");

    match route {
        SgRoute::Mcp(route) => {
            assert_eq!(route.route_name, "mcp");
            assert_eq!(route.transport, SgMcpTransport::StreamableHttp);
            assert_eq!(route.timeout_mode, McpTimeoutMode::Disabled);
            assert_eq!(route.session_affinity, McpSessionAffinity::McpSession);
            assert_eq!(route.backends.len(), 1);
        }
        SgRoute::Http(_) => panic!("MCPRoute should parse as SgRoute::Mcp"),
    }
}
