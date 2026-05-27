#!/usr/bin/env python3
"""一个进程内启动三个 mock HTTP 服务，配合 hai-process-mix.wasm 联调：

- 18091  ac-service：API Key 鉴权（返回 ApiKeyRecord JSON）
- 18092  asset-service：资产查询（返回 AssetRecord JSON）
- 18099  upstream-echo：扮演 hai-gw-server / 任意业务上游，回声请求头与方法

每个服务都在独立线程内跑标准库 http.server.HTTPServer，无第三方依赖。
启动方式：python3 mock_backends.py
"""

import json
import threading
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer


# 简单的 API Key → ApiKeyRecord 字典（按 demo 需要可继续扩）
API_KEYS = {
    "demo-key": {
        "app_id": "demo-app",
        # 包含 demo-asset 在内，hai 才会放行
        "asset_ids": ["demo-asset"],
        "allow_ips": [],
        "deny_ips": [],
        "allow_mac_addrs": [],
        "deny_mac_addrs": [],
        # ISO 8601 UTC，预留够久
        "expired_at": (datetime.now(tz=timezone.utc) + timedelta(days=3650)).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
}

ASSETS = {
    "demo-asset": {
        "asset_id": "demo-asset",
        "asset_type": "tool",
        "asset_status": "published",
        # 让 hai 走"分支 A 转发"：写 Hai-Upstream-URL 等头，路由到 backend
        "runtime_endpoint": "http://upstream-echo.demo/echo",
        "runtime_endpoint_method": ["POST"],
        "asset_content": None,
        "asset_url": None,
        "max_concurrent": 16,
        "timeout_sec": 30,
        "qps_limit": 100,
        "asset_secret_params": [],
        "asset_secret_values": {},
        "allowed_output_targets": [],
    }
}


class AcHandler(BaseHTTPRequestHandler):
    # 静默日志（避免刷屏，可改成 print 来调试）
    def log_message(self, format, *args):
        return

    def do_GET(self):
        if self.path != "/ai-agent/internal/v1/ac/auth":
            self.send_response(404)
            self.end_headers()
            return
        # hai 把 API Key 放到 hai-api-key 请求头
        api_key = self.headers.get("hai-api-key") or self.headers.get("Hai-Api-Key")
        rec = API_KEYS.get((api_key or "").strip())
        if not rec:
            self.send_response(401)
            self.send_header("Content-Type", "application/json")
            body = json.dumps({"code": "invalid_api_key", "message": "unknown key"}).encode()
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        body = json.dumps(rec).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class AssetHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return

    def do_GET(self):
        # 路径形如 /ai-agent/internal/v1/am/assets/<asset_id>
        prefix = "/ai-agent/internal/v1/am/assets/"
        if not self.path.startswith(prefix):
            self.send_response(404)
            self.end_headers()
            return
        asset_id = self.path[len(prefix):].split("?", 1)[0].split("/", 1)[0]
        rec = ASSETS.get(asset_id)
        if not rec:
            self.send_response(404)
            self.end_headers()
            return
        body = json.dumps(rec).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class EchoHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return

    def _echo(self):
        # 回声：把请求头（重点是 Hai-* / x-*）放在响应 JSON 内，便于断言注入
        seen_headers = {k.lower(): v for k, v in self.headers.items()}
        payload = {
            "method": self.command,
            "path": self.path,
            "headers": seen_headers,
        }
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("X-Upstream-Echo", "ok")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self._echo()

    def do_POST(self):
        # 排掉 body（不影响 echo）
        _ = self.rfile.read(int(self.headers.get("Content-Length") or 0) or 0)
        self._echo()


def serve(port, handler):
    httpd = HTTPServer(("127.0.0.1", port), handler)
    print(f"[mock] listen 127.0.0.1:{port} ({handler.__name__})")
    httpd.serve_forever()


def main():
    threads = [
        threading.Thread(target=serve, args=(18091, AcHandler), daemon=True),
        threading.Thread(target=serve, args=(18092, AssetHandler), daemon=True),
        threading.Thread(target=serve, args=(18099, EchoHandler), daemon=True),
    ]
    for t in threads:
        t.start()
    print("[mock] all three services up; Ctrl-C to stop")
    for t in threads:
        t.join()


if __name__ == "__main__":
    main()
