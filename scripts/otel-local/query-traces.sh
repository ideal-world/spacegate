#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_CLICKHOUSE_NAME:-spacegate-clickhouse}"

docker exec "$NAME" clickhouse-client --database otel --query "
SELECT
  Timestamp,
  TraceId,
  SpanId,
  ParentSpanId,
  SpanName,
  Duration,
  StatusCode,
  SpanAttributes['http.status_code'] AS http_status_code,
  SpanAttributes['request_id'] AS request_id
FROM otel_traces
ORDER BY Timestamp DESC
LIMIT 20
FORMAT Vertical
"
