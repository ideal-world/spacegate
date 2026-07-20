#!/bin/sh
set -eu

: "${CONFIG:?CONFIG is required, for example: k8s:spacegate or file:/etc/spacegate}"

./admin-server -H 127.0.0.1 -p 9081 -c "$CONFIG" &
admin_pid="$!"

nginx -g 'daemon off;' &
nginx_pid="$!"

cleanup() {
  kill -TERM "$admin_pid" "$nginx_pid" 2>/dev/null || true
}

trap cleanup INT TERM

while :; do
  if ! kill -0 "$admin_pid" 2>/dev/null; then
    set +e
    wait "$admin_pid"
    status="$?"
    set -e
    cleanup
    exit "$status"
  fi
  if ! kill -0 "$nginx_pid" 2>/dev/null; then
    set +e
    wait "$nginx_pid"
    status="$?"
    set -e
    cleanup
    exit "$status"
  fi
  sleep 1
done
