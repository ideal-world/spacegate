#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_OTEL_COLLECTOR_NAME:-spacegate-otel}"
docker logs "$NAME" 2>&1 | rg 'otelcol.signal|http_access|http access log|Logs|Traces|Metrics|LogRecord|Span|http.server'
