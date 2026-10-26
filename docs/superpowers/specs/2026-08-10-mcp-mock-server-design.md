# MCP 双协议本地 Mock 设计

## 目标

提供一个可本地或测试环境启动的有状态 MCP mock，以验证 Spacegate 对 Streamable HTTP 和 Legacy SSE upstream 的转发行为。mock 不实现 MCP SDK，也不承担生产 MCP server 的职责。

## 端点与行为

一个进程默认监听 `127.0.0.1:3001`，同时提供以下端点：

- `POST /mcp`：处理 `initialize`、`tools/list` 与 `tools/call`。初始化响应带 `Mcp-Session-Id`；`tools/call` 可按请求参数返回 JSON 或两帧 SSE。
- `GET /mcp`：返回保持打开的两帧 SSE，用于验证 Streamable HTTP 下行流。
- `DELETE /mcp`：要求已知 `Mcp-Session-Id`，删除会话并返回 `204`。
- `GET /sse`：创建 Legacy SSE session，立即写入 `event: endpoint`，并保持连接等待 message 请求。
- `POST /message?sessionId=<id>`：对已建立的 Legacy SSE session 返回 `202`，再通过该 session 原来的 SSE 连接写入对应 JSON-RPC 响应；未知 session 返回 `404`。

`tools/list` 返回固定工具清单；`tools/call` 返回请求 ID 对应的固定 `mock.echo` 结果。无需实现 tools、resources 或 prompts 的完整 MCP 语义。

## 状态与边界

Streamable HTTP session 和 Legacy SSE sender 存在进程内 map 中，并以固定 TTL 清理。Legacy SSE 连接断开后的 send 失败会删除关联 session。mock 仅模拟 upstream 协议语义，不校验认证、不提供 TLS、不持久化 session，也不修改 Spacegate 配置。

## 验收

- Legacy SSE 建连后能接收 endpoint 事件；POST message 会在同一连接收取 JSON-RPC 响应。
- Streamable HTTP initialize、tools/list、tools/call SSE、GET SSE 和 DELETE session 均可响应。
- 未知 session 明确返回 `404`。
- 使用现有 workspace 的 `tokio`、`hyper`、`http-body-util`、`serde_json` 与 `futures-util`，不新增依赖。
