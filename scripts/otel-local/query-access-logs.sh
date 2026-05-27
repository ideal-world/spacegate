#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_CLICKHOUSE_NAME:-spacegate-clickhouse}"

docker exec "$NAME" clickhouse-client --database otel --query "
SELECT
  Timestamp,
  Body,
  SeverityText,
  LogAttributes['event'] AS event,
  LogAttributes['gateway'] AS gateway,
  LogAttributes['method'] AS method,
  LogAttributes['path'] AS path,
  LogAttributes['authority'] AS authority,
  LogAttributes['client_ip'] AS client_ip,
  LogAttributes['x_forwarded_for'] AS x_forwarded_for,
  LogAttributes['user_agent'] AS user_agent,
  LogAttributes['downstream_remote_address'] AS downstream_remote_address,
  LogAttributes['route_name'] AS route_name,
  LogAttributes['upstream_host'] AS upstream_host,
  LogAttributes['trace_id'] AS trace_id,
  LogAttributes['status_code'] AS status_code,
  LogAttributes['request_id'] AS request_id,
  LogAttributes['duration_ms'] AS duration_ms,
  LogAttributes['bytes_received'] AS bytes_received,
  LogAttributes['bytes_sent'] AS bytes_sent,
  LogAttributes['telemetry'] AS telemetry,
  JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id') AS ai_asset_id,
  JSONExtractString(LogAttributes['telemetry'], 'ai.asset_type') AS ai_asset_type,
  JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens') AS ai_total_tokens,
  JSONExtractString(LogAttributes['telemetry'], 'mcp.server') AS mcp_server,
  JSONExtractString(LogAttributes['telemetry'], 'mcp.tool') AS mcp_tool,
  JSONExtractString(LogAttributes['telemetry'], 'mcp.success') AS mcp_success,
  JSONExtractString(LogAttributes['telemetry'], 'auth.app_id') AS auth_app_id,
  JSONExtractString(LogAttributes['telemetry'], 'auth.api_key_hash') AS auth_api_key_hash
FROM otel_logs
WHERE LogAttributes['event'] = 'http_access'
ORDER BY Timestamp DESC
LIMIT 20
FORMAT Vertical
"
