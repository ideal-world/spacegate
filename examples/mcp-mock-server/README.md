# MCP 本地 Mock Server

这是给 Spacegate MCPRoute 集成、预发和故障复现使用的有状态 upstream mock。一个进程同时提供 Streamable HTTP 与 Legacy SSE，不需要 MCP SDK 或外部服务。

## 启动

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace/spacegate
cargo run -p mcp-mock-server
```

默认监听 `http://127.0.0.1:3001`。需要让容器或其他主机访问时，显式指定地址：

```bash
cargo run -p mcp-mock-server -- --addr 0.0.0.0:3001
```

## 协议端点

| 协议 | 端点 | 行为 |
| --- | --- | --- |
| Streamable HTTP | `POST /mcp` | `initialize`、`tools/list`、`tools/call` |
| Streamable HTTP | `GET /mcp` | 返回 notification SSE 与 keepalive |
| Streamable HTTP | `DELETE /mcp` | 使用 `Mcp-Session-Id` 删除初始化会话 |
| Legacy SSE | `GET /sse` | 保持 SSE 连接，并先推送 endpoint event |
| Legacy SSE | `POST /message?sessionId=<id>` | 返回 `202`，随后从已打开的 SSE 连接返回 JSON-RPC 结果 |

`tools/list` 总是返回 `mock.echo`。`tools/call` 带 `{"stream":true}` 时会产生两帧 SSE：JSON-RPC result 与 `data: [DONE]`。

## 直连验证

Streamable HTTP 初始化会返回 `Mcp-Session-Id: streamable-1`：

```bash
curl -i -X POST http://127.0.0.1:3001/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
```

使用返回的 session 调用工具和关闭会话：

```bash
curl -i -X POST http://127.0.0.1:3001/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Mcp-Session-Id: streamable-1' \
  --data '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

curl -i -N -X POST http://127.0.0.1:3001/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"stream":true}}'

curl -i -X DELETE http://127.0.0.1:3001/mcp \
  -H 'Mcp-Session-Id: streamable-1'
```

Legacy SSE 必须使用两个终端。终端 A 保持连接并记录 `event: endpoint` 的 `data` 路径：

```bash
curl -N http://127.0.0.1:3001/sse
```

它会先输出 `data: /message?sessionId=legacy-2`（session 编号取决于当前进程此前建立的会话）。终端 B 将该路径原样用于 POST：

```bash
curl -i -X POST 'http://127.0.0.1:3001/message?sessionId=legacy-2' \
  -H 'Content-Type: application/json' \
  --data '{"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}'
```

终端 B 应收到 `202 Accepted`；终端 A 的**原连接**应收到 `event: message` 与对应 JSON-RPC result。这一步覆盖 Legacy SSE 最关键的异步回写路径。

## MCPRoute 接入片段

下面仅说明 mock backend 地址；路由对象请按你的配置源（文件或 K8S）写入，mock 不会改动网关配置。

Streamable HTTP：

```toml
kind = "MCPRoute"
route_name = "mock-streamable"
transport = "streamable_http"
path = "/mcp"
timeout_mode = "disabled"

[[backends]]
host = { kind = "Host", host = "127.0.0.1" }
port = 3001
protocol = "http"
weight = 1
```

Legacy SSE：

```toml
kind = "MCPRoute"
route_name = "mock-legacy"
transport = "legacy_sse"
path = "/unused"
legacy_sse = { sse_path = "/sse", message_path = "/message" }
timeout_mode = "disabled"

[[backends]]
host = { kind = "Host", host = "127.0.0.1" }
port = 3001
protocol = "http"
weight = 1
```

若 Spacegate 运行在 Docker 容器中，`127.0.0.1` 指向容器自身；Docker Desktop 通常应改为 `host.docker.internal`。Kubernetes 请将 mock 暴露为 Service 或使用节点可达地址。

## 边界

mock 只覆盖网关需要的 JSON-RPC、session header 与 SSE 传输行为，不实现认证、TLS、资源、prompt、真实工具执行或跨进程 session 持久化。它不能替代真实 MCP server 的兼容性验证。
