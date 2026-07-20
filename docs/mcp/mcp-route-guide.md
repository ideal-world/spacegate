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

MCPRoute 默认 `session_affinity = "mcp_session"`。多 backend 时会优先按 `Mcp-Session-Id` 做 hash 选择；缺少该 header 时回退到客户端 IP hash；单 backend 时直接使用该 backend。

如果不需要会话粘性，可以设置：

```toml
session_affinity = "none"
```

## 插件限制

MCPRoute 不解析 JSON-RPC body。建议只挂载不需要缓存、聚合或完整读取 streaming body 的插件。

对于 `text/event-stream` 响应，代理层保持流式转发，不应使用会 collect body 的检查类插件。

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
