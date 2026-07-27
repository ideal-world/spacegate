# MCPRoute 代理指南

Spacegate 的 MCPRoute 用于透明代理外部已有 MCP 服务。首版只做协议代理和网关治理，不实现 MCP server/client，也不解析或执行 JSON-RPC 的 tool、resource、prompt。

运行时 MCPRoute 会编译为现有 HTTP streaming route，因此继续复用 HTTP/1.1、HTTP/2、SSE、backend、插件和热更新链路。

## Streamable HTTP

`transport = "streamable_http"` 时，`path` 会同时生成两个匹配：

- `GET {path}`：用于服务端事件流响应。
- `POST {path}`：用于 JSON-RPC 请求/响应。

MCP 相关请求头会按普通 HTTP header 透传，包括：

- `Accept`
- `Content-Type`
- `Authorization`
- `MCP-Protocol-Version`
- `Mcp-Session-Id`
- `Last-Event-ID`

网关不会解析 JSON-RPC body，但会校验 Streamable HTTP 的传输 envelope：

- `GET` 必须接受 `text/event-stream`；
- `POST` 必须使用 `Content-Type: application/json`，并接受 `application/json` 与 `text/event-stream`；
- 不符合要求的请求会在到达 upstream 前返回 `406` 或 `415`，且不会记录请求头值。

示例：

```toml
kind = "MCPRoute"
route_name = "mcp"
hostnames = ["ai.example.com"]
transport = "streamable_http"
path = "/mcp"
timeout_mode = "disabled"
session_affinity = "mcp_session"

[[backends]]
host = { kind = "Host", host = "mcp-server.default.svc.cluster.local" }
port = 8080
protocol = "http"
weight = 1
```

## Legacy SSE

`transport = "legacy_sse"` 时必须使用显式 SSE 和 message 路径。两个路径会转发到同一组 backend：

- `GET legacy_sse.sse_path`
- `POST legacy_sse.message_path`

示例：

```toml
kind = "MCPRoute"
route_name = "mcp-sse"
transport = "legacy_sse"
path = "/mcp"
legacy_sse = { sse_path = "/sse", message_path = "/message" }
timeout_mode = "disabled"
session_affinity = "mcp_session"

[[backends]]
host = { kind = "Host", host = "127.0.0.1" }
port = 3001
protocol = "http"
weight = 1
```

## Timeout

MCPRoute 默认 `timeout_mode = "disabled"`，不会套整体请求超时，适合长连接和 SSE 流式响应。

如果需要沿用普通 HTTP 请求超时，可以设置：

```toml
timeout_mode = "request"
```

此时 backend 或 rule 上的 `timeout_ms` 继续按现有 HTTPRoute 语义生效。

## Session Affinity

MCPRoute 默认 `session_affinity = "mcp_session"`。初始化请求按客户端 IP hash 选择 backend；upstream 返回 `Mcp-Session-Id` 后，当前 Spacegate 进程会记录 `session_id -> backend_index`，后续请求优先回到创建 session 的 backend。未知或过期 session 回退到 session hash；缺少 session header 时回退到客户端 IP hash；单 backend 时直接使用该 backend。

该映射是有容量和 TTL 限制的进程内状态：它适用于单个 Spacegate 进程，或入口负载均衡已将同一客户端固定到同一 Spacegate Pod 的部署。多 Pod 且没有入口粘性时，需要 upstream MCP server 自身共享 session，或另行设计共享 session-store 扩展；不得静默复用配置存储 Redis。分布式 Redis session store 属于独立的公开接口变更，必须同时设计模型、K8S CRD、SDK、前端和失败策略。

如果不需要会话粘性，可以设置：

```toml
session_affinity = "none"
```

## 插件行为

MCPRoute 不解析 JSON-RPC body，也不会因为插件读取或改写 MCP body 而拒绝、禁用或告警。鉴权、限流、header 改写、body 解析与改写、敏感词过滤、token 用量统计和审计插件继续按普通 HTTPRoute 的方式执行。

对于普通 JSON 响应，Proxy-Wasm host 可以继续使用有界的完整 body 处理；对于 `text/event-stream` 响应，host 必须逐 chunk 调用响应 body hook，并保持增量转发。

Spacegate 支持 Hai 插件既有的流式 response hook：插件可在每个 SSE chunk 上累计 token 用量、过滤敏感输出，并在 EOF 回调中完成最终用量上报；不需要额外声明 MCP 专用插件能力。

每个 MCPRoute 请求会写入 access-log telemetry：`mcp.transport`、`mcp.route_type`、`mcp.session_id_present`。其中只记录 session header 是否存在，不记录 session ID 或认证信息。

## 当前完成度与补充计划

当前代码已具备一等 `MCPRoute` 配置模型、Admin 表单/SDK、文件配置兼容读取、K8S CRD 的 CRUD/watch/retrieve、Streamable HTTP 与 Legacy SSE 编译、禁用整体请求超时，以及按 `Mcp-Session-Id`（缺失时按客户端 IP）选择 backend。运行时仍复用 HTTP streaming proxy，不会解析 JSON-RPC body。

仍需完成以下正式代理增强后，才能把 MCPRoute 的“协议治理和观测”边界完全闭合：

1. **streaming host 支持**：为 Proxy-Wasm host 增加 SSE 逐 chunk body hook，保留现有插件行为，不引入 MCP 专用能力声明。
2. **端到端回归**：增加 mock MCP upstream，验证 Streamable HTTP 的 GET SSE 首个 chunk 不被缓存、POST JSON/事件流透传、Legacy SSE 双路径、30 秒以上连接、header 完整透传、session 粘性与无 session 的 IP 回退。
3. **K8S 真实集成测试**：现有 CRD/转换测试覆盖对象模型；还需要 kind/k3d 环境安装 CRD 后执行 watch、更新和删除 MCPRoute 的 reload 回归。

建议实施顺序是 1 → 2 → 3；当前已具备 transport 校验与安全观测，后两项补齐生产回归和运维防误用能力。

## K8S MCPRoute

K8S 配置使用 `spacegate.idealworld.group/v1` 的 `MCPRoute`，`parentRefs` 指向 `Gateway`：

部署前需要安装 MCPRoute CRD，并确保 Spacegate ServiceAccount 具有 `mcproutes` 的 `get/list/watch` 权限：

```bash
kubectl apply -f resource/kube-manifests/spacegate-mcproute.yaml
```

```yaml
apiVersion: spacegate.idealworld.group/v1
kind: MCPRoute
metadata:
  name: mcp
spec:
  parentRefs:
    - kind: Gateway
      name: default
  hostnames:
    - ai.example.com
  transport: streamable_http
  path: /mcp
  timeout_mode: disabled
  session_affinity: mcp_session
  backend_refs:
    - kind: ExternalHttp
      name: mcp-server.default.svc.cluster.local
      port: 8080
      weight: 1
```
