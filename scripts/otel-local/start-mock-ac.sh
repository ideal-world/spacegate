#!/usr/bin/env bash
set -euo pipefail

HOST="${SPACEGATE_MOCK_HOST:-127.0.0.1}"
PORT="${SPACEGATE_MOCK_PORT:-18080}"

python3 - "$HOST" "$PORT" <<'PY'
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

host = sys.argv[1]
port = int(sys.argv[2])

class Handler(BaseHTTPRequestHandler):
    def _send(self):
        body = json.dumps({
            "role": "ac",
            "endpoint": f"{self.command} {self.path}  (header Hai-Api-Key)",
            "known_keys": ["demo-key", "demo-key-empty-expiry", "expired-key", "other-app-key"],
        }).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self._send()

    def do_POST(self):
        self._send()

    def log_message(self, fmt, *args):
        print(f"{self.address_string()} - {fmt % args}", flush=True)

server = ThreadingHTTPServer((host, port), Handler)
print(f"mock ac listening on http://{host}:{port}", flush=True)
server.serve_forever()
PY
