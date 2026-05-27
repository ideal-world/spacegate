#!/usr/bin/env bash
set -euo pipefail

NAME="${SPACEGATE_OTEL_COLLECTOR_NAME:-spacegate-otel}"
docker logs -f "$NAME"
