#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_CLICKHOUSE_NAME:-spacegate-clickhouse}"

docker exec "$NAME" clickhouse-client --database otel --query "
SELECT 'otel_logs' AS table, count() AS rows FROM otel_logs
UNION ALL
SELECT 'otel_traces' AS table, count() AS rows FROM otel_traces
UNION ALL
SELECT 'otel_metrics_sum' AS table, count() AS rows FROM otel_metrics_sum
UNION ALL
SELECT 'otel_metrics_histogram' AS table, count() AS rows FROM otel_metrics_histogram
FORMAT PrettyCompact
"
