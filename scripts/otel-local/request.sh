#!/usr/bin/env bash
set -euo pipefail

URL="${1:-http://127.0.0.1:9000/}"
curl -v "$URL"
