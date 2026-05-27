#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="${SPACEGATE_OTEL_DIR:-/tmp/spacegate-otel}"
CONFIG_DIR="$BASE_DIR/config"
GATEWAY_DIR="$CONFIG_DIR/gateway/local"

mkdir -p "$GATEWAY_DIR/route"

cat > "$BASE_DIR/otel-collector.yaml" <<'YAML'
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

processors:
  batch:

exporters:
  clickhouse:
    endpoint: tcp://spacegate-clickhouse:9000?dial_timeout=10s
    database: otel
    async_insert: true
    compress: lz4
    create_schema: true
    logs_table_name: otel_logs
    traces_table_name: otel_traces
    timeout: 5s
    metrics_tables:
      gauge:
        name: otel_metrics_gauge
      sum:
        name: otel_metrics_sum
      summary:
        name: otel_metrics_summary
      histogram:
        name: otel_metrics_histogram
      exponential_histogram:
        name: otel_metrics_exp_histogram
    retry_on_failure:
      enabled: true
      initial_interval: 5s
      max_interval: 30s
      max_elapsed_time: 300s

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [clickhouse]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [clickhouse]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [clickhouse]
YAML

cat > "$CONFIG_DIR/config.json" <<'JSON'
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
      "enabled": false,
      "export_interval_ms": 60000
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
JSON

cat > "$GATEWAY_DIR/config.json" <<'JSON'
{
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
  }
}
JSON

cat > "$GATEWAY_DIR/route/root.json" <<'JSON'
{
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
JSON

echo "Prepared local OTEL config under $BASE_DIR"
echo "SpaceGate config: $CONFIG_DIR"
echo "Collector config: $BASE_DIR/otel-collector.yaml"
