# SpaceGate OTEL 三信号说明

本文说明 SpaceGate 当前接入 OpenTelemetry 后，`logs`、`traces`、`metrics` 三类数据分别如何上报、数据结构大致是什么样、以及分别适合哪些审计和监控需求。

## 1. 当前链路

当前本地验证链路：

```text
SpaceGate
  -> OTLP gRPC
  -> OpenTelemetry Collector
  -> ClickHouse
```

本地配置位置：

```text
/tmp/spacegate-otel/config/config.json
/tmp/spacegate-otel/otel-collector.yaml
```

SpaceGate OTLP endpoint：

```json
"otlp_endpoint": "http://127.0.0.1:4317",
"protocol": "grpc"
```

Collector 接收：

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318
```

Collector 写入 ClickHouse：

```yaml
exporters:
  clickhouse:
    endpoint: tcp://spacegate-clickhouse:9000?dial_timeout=10s
    database: otel
    create_schema: true
```

## 2. Logs：审计明细

### 上报方式

SpaceGate 使用 Rust `tracing::info!` 生成结构化日志事件，再通过 OpenTelemetry logs exporter 走 OTLP 推送给 Collector。

当前每个请求完成时，网关会生成一条 `info` 级别的 access log：

```text
event = "http_access"
```

插件可以把业务审计字段写入请求级 `TelemetryContext`，请求结束时统一进入 access log 的 `telemetry` 字段。

### 数据结构

ClickHouse 表：

```text
otel_logs
```

常见字段形态：

```text
Timestamp
TraceId
SpanId
SeverityText
Body
LogAttributes
ResourceAttributes
```

其中 `LogAttributes` 是 key/value map。当前 SpaceGate access log 关键字段：

```text
LogAttributes['event']          = 'http_access'
LogAttributes['gateway']        = 'local'
LogAttributes['method']         = 'GET'
LogAttributes['path']           = '/'
LogAttributes['host']           = '127.0.0.1:9000'
LogAttributes['authority']      = '127.0.0.1:9000'
LogAttributes['client_ip']      = '127.0.0.1'
LogAttributes['x_forwarded_for']
LogAttributes['user_agent']     = 'curl/8.7.1'
LogAttributes['downstream_remote_address'] = '127.0.0.1:xxxxx'
LogAttributes['route_name']     = 'local-test'
LogAttributes['upstream_host']  = '127.0.0.1'
LogAttributes['trace_id']       = '4bf92f3577b34da6a3ce929d0e0e4736'
LogAttributes['status_code']    = '200'
LogAttributes['request_id']     = '...'
LogAttributes['peer_addr']      = '127.0.0.1:xxxxx'
LogAttributes['duration_ms']    = '...'
LogAttributes['bytes_received']
LogAttributes['bytes_sent']
LogAttributes['request_body_size']
LogAttributes['response_body_size']
LogAttributes['telemetry']      = '{"ai.asset_id":"...","ai.total_tokens":"37"}'
```

`client_ip` 优先取 `X-Forwarded-For` 的第一个 IP，缺失时退回 TCP peer IP。MAC 地址不是 HTTP 请求语义的一部分，网关在代理、NAT、K8s 场景下无法可靠获得；如果审计确实需要，只能由上游可信组件或插件以业务字段写入 `telemetry`。

插件写入的审计字段在 `telemetry` JSON 里。例如：

```json
{
  "ai.asset_id": "deepseek-chat",
  "ai.asset_type": "model",
  "ai.prompt_tokens": "24",
  "ai.completion_tokens": "13",
  "ai.total_tokens": "37",
  "mcp.server": "search-service",
  "mcp.tool": "web_search",
  "auth.app_id": "demo-app"
}
```

查询示例：

```sql
SELECT
  Timestamp,
  LogAttributes['request_id'] AS request_id,
  LogAttributes['path'] AS path,
  LogAttributes['status_code'] AS status_code,
  JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id') AS asset_id,
  toUInt64OrZero(JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens')) AS total_tokens,
  JSONExtractString(LogAttributes['telemetry'], 'mcp.tool') AS mcp_tool
FROM otel_logs
WHERE LogAttributes['event'] = 'http_access'
ORDER BY Timestamp DESC
LIMIT 20;
```

### 适合的需求

Logs 适合做**审计明细**：

- 每次接口调用记录
- 请求状态码、耗时、request_id
- 应用、API Key 摘要、租户信息
- 大模型 asset_id、token 用量
- MCP server/tool 调用信息
- 错误码、失败原因
- 审计中心按请求维度查询和导出

### 不适合的需求

Logs 不适合作为高频实时监控聚合的唯一数据源。虽然可以统计，但大量 JSON 提取和明细扫描成本较高。高频监控建议用 metrics。

## 3. Traces：调用链路

### 上报方式

SpaceGate 在请求入口创建 HTTP server span。插件内部使用 `tracing` 打出的事件可以挂到当前 span 上。OpenTelemetry traces exporter 通过 OTLP 推送给 Collector。

### 数据结构

ClickHouse 表：

```text
otel_traces
```

常见字段形态：

```text
Timestamp
TraceId
SpanId
ParentSpanId
SpanName
ServiceName
Duration
StatusCode
SpanAttributes
ResourceAttributes
Events
```

当前请求 span 示例字段：

```text
SpanName = 'http.server.request'
SpanAttributes['http.method'] = 'GET'
SpanAttributes['http.path'] = '/'
SpanAttributes['http.host'] = '127.0.0.1:9000'
SpanAttributes['http.protocol'] = 'HTTP/1.1'
SpanAttributes['http.status_code'] = '200'
SpanAttributes['request_id'] = '...'
SpanAttributes['peer_addr'] = '127.0.0.1:xxxxx'
SpanAttributes['duration_ms'] = '...'
```

查询示例：

```sql
SELECT
  Timestamp,
  TraceId,
  SpanId,
  ParentSpanId,
  SpanName,
  Duration,
  StatusCode,
  SpanAttributes['http.status_code'] AS http_status_code,
  SpanAttributes['request_id'] AS request_id
FROM otel_traces
ORDER BY Timestamp DESC
LIMIT 20;
```

### 适合的需求

Traces 适合做**链路诊断**：

- 一次请求经过了哪些内部阶段
- 哪个插件或后端调用耗时高
- 请求失败时定位失败发生在哪一段
- 根据 `TraceId` 把 logs 和 spans 串起来
- 抽样分析慢请求和异常请求

### 不适合的需求

Traces 不适合作为完整审计账本。生产环境通常会采样，例如 1%、0.1% 或 parent-based sampling。审计要求完整性时，应以 logs 为准。

## 4. Metrics：聚合监控

### 上报方式

SpaceGate 使用 OpenTelemetry metrics SDK 定期导出指标。当前本地配置里有：

```json
"metrics": {
  "enabled": true,
  "export_interval_ms": 5000
}
```

这表示每 5 秒导出一次当前指标数据。即使没有新请求，累计型指标也可能周期性写入 ClickHouse，所以 metrics 表行数会持续增加。

### 数据结构

ClickHouse 表通常包括：

```text
otel_metrics_sum
otel_metrics_histogram
otel_metrics_gauge
otel_metrics_summary
otel_metrics_exp_histogram
```

当前 SpaceGate 请求级指标包括：

```text
http.server.requests
http.server.errors
http.server.errors.4xx
http.server.errors.5xx
http.server.active_requests
http.server.request.duration
http.server.request.body.size
http.server.response.body.size
```

指标属性使用低基数字段：

```text
gateway
http.request.method
http.response.status_code
network.protocol.name
network.protocol.version
```

示例含义：

```text
http.server.requests
  类型：Counter
  作用：请求总量

http.server.request.duration
  类型：Histogram
  单位：s
  作用：请求耗时分布，可计算 P50/P95/P99

http.server.errors.5xx
  类型：Counter
  作用：服务端错误数量

http.server.active_requests
  类型：UpDownCounter
  作用：当前活跃请求数
```

### 适合的需求

Metrics 适合做**监控和告警**：

- QPS
- 错误率
- P95/P99 延迟
- 活跃请求数
- 请求/响应大小分布
- 4xx/5xx 趋势
- 容量规划
- SLO/SLA 面板

### 不适合的需求

Metrics 不适合做逐请求审计：

- 不包含完整 request_id
- 不应该带 api_key、user_id、asset_id 这类高基数字段
- 不记录每次请求的完整业务明细
- 周期性导出会产生重复时间序列点，不能用行数代表请求数

## 5. 三者对比

| 信号 | 粒度 | 数据完整性 | 成本 | 主要用途 | ClickHouse 表 |
| --- | --- | --- | --- | --- | --- |
| Logs | 单请求明细 | 高 | 中到高 | 审计、账单、问题回溯 | `otel_logs` |
| Traces | 调用链路 | 取决于采样 | 中 | 慢请求诊断、链路分析 | `otel_traces` |
| Metrics | 聚合数据 | 聚合后数据 | 低到中 | 监控、告警、趋势 | `otel_metrics_*` |

## 6. 审计中心推荐使用方式

审计中心建议以 logs 为主：

```text
otel_logs
  WHERE LogAttributes['event'] = 'http_access'
```

核心查询字段：

```text
Timestamp
LogAttributes['request_id']
LogAttributes['gateway']
LogAttributes['method']
LogAttributes['path']
LogAttributes['status_code']
LogAttributes['duration_ms']
LogAttributes['telemetry']
```

插件业务字段从 `telemetry` JSON 里解析：

```sql
JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id')
JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens')
JSONExtractString(LogAttributes['telemetry'], 'mcp.tool')
```

如果审计中心需要高频统计，例如按模型统计 token：

```sql
SELECT
  JSONExtractString(LogAttributes['telemetry'], 'ai.asset_id') AS asset_id,
  sum(toUInt64OrZero(JSONExtractString(LogAttributes['telemetry'], 'ai.total_tokens'))) AS total_tokens
FROM otel_logs
WHERE LogAttributes['event'] = 'http_access'
GROUP BY asset_id;
```

生产上建议对常用字段建 ClickHouse 物化视图，把 JSON 字段抽成列，提升查询性能。

## 7. 监控系统推荐使用方式

监控面板建议使用 metrics：

- `http.server.requests` 计算请求量
- `http.server.errors` / `http.server.requests` 计算错误率
- `http.server.request.duration` 计算延迟分位数
- `http.server.active_requests` 观察并发压力

本地测试阶段如果只验证审计入库，可以关闭 metrics：

```json
"metrics": {
  "enabled": false,
  "export_interval_ms": 60000
}
```

生产建议使用较低频率：

```json
"metrics": {
  "enabled": true,
  "export_interval_ms": 30000
}
```

如果 metrics 长期写 ClickHouse，建议配置 TTL 或单独存入更适合时序数据的系统。

## 8. 推荐配置策略

### 本地审计验证

```text
logs: enabled
traces: enabled, sample_ratio = 1.0
metrics: disabled
```

### 生产审计

```text
logs: enabled
traces: enabled, parent-based sampling
metrics: enabled, 30s 或 60s interval
```

### 生产监控

```text
metrics: enabled
logs: only access/audit logs
traces: sampling
```

## 9. 总结

- **Logs 是审计主数据**：每个请求一条 `http_access`，插件审计字段在 `telemetry` JSON 中。
- **Traces 是诊断数据**：用 `TraceId` 追踪一次请求的链路和耗时。
- **Metrics 是监控数据**：周期性聚合导出，用于 QPS、错误率、延迟、告警。
- 不要把业务审计字段作为 metrics label。
- 不要把 traces 当完整审计账本。
- 审计中心主要查 `otel_logs`，监控系统主要用 `otel_metrics_*`。
