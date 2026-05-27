# SpaceGate OTEL 本地联调启动手册

本目录用于本地验证 SpaceGate 通过 OTLP 向 OpenTelemetry Collector 推送 `traces`、`metrics`、`logs`，并由 Collector 写入 ClickHouse。

默认端口：

- SpaceGate HTTP 入口：`9000`
- SpaceGate API 端口：`9876`
- Mock AC 后端：`18080`
- OTLP gRPC：`4317`
- OTLP HTTP：`4318`
- ClickHouse HTTP：`28123`
- ClickHouse Native：`29000`

默认运行目录：

```bash
/tmp/spacegate-otel
```

## 0. 进入项目目录

作用：确保后续脚本都从 SpaceGate 仓库根目录执行。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
```

## 1. 生成本地配置

作用：生成 Collector 配置、SpaceGate 主配置、gateway 配置和 route 配置。

```bash
scripts/otel-local/prepare-config.sh
```

生成后的关键文件：

```bash
/tmp/spacegate-otel/otel-collector.yaml
/tmp/spacegate-otel/config/config.json
/tmp/spacegate-otel/config/gateway/local/config.json
/tmp/spacegate-otel/config/gateway/local/route/root.json
```

## 2. 终端 1：启动 ClickHouse

作用：启动本地 ClickHouse，Collector 会把三信号写入 `otel` 数据库。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/start-clickhouse.sh
```

如果端口被占用，先检查已有 ClickHouse：

```bash
docker ps --filter name=spacegate-clickhouse
```

停止已有 ClickHouse：

```bash
docker rm -f spacegate-clickhouse
```

## 3. 终端 2：启动 OTEL Collector

作用：启动 OpenTelemetry Collector，接收 SpaceGate 推送的 OTLP 三信号，并写入 ClickHouse。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/start-collector.sh
```

如果端口被占用，先停止已有 Collector：

```bash
docker rm -f spacegate-otel
```

## 4. 终端 3：启动 Mock AC 后端

作用：启动本地模拟后端服务，监听 `127.0.0.1:18080`，SpaceGate 会把请求转发到它。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/start-mock-ac.sh
```

单独验证 Mock AC：

```bash
curl -v http://127.0.0.1:18080/
```

## 5. 终端 4：启动 SpaceGate

作用：使用 `/tmp/spacegate-otel/config` 文件配置启动 SpaceGate，并启用 OTLP traces、metrics、logs。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/start-spacegate.sh
```

等看到 SpaceGate 正常启动后，再执行请求验证。

如果你想临时打开更详细日志：

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
RUST_LOG=trace scripts/otel-local/start-spacegate.sh
```

## 6. 终端 5：发送测试请求

作用：向 SpaceGate 的 `9000` 端口发送请求，触发路由转发、trace、metrics 和 access log。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/request.sh
```

等价命令：

```bash
curl -v http://127.0.0.1:9000/
```

预期响应：

```text
HTTP/1.0 200 OK
content-type: application/json
x-request-id: ...
```

响应体里应包含：

```json
{"role":"ac"}
```

## 7. 查看 Collector 全量输出

作用：实时查看 Collector 运行日志。入库模式下这里主要看是否有 ClickHouse 写入错误。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/logs-collector.sh
```

## 8. 查询 ClickHouse 入库行数

作用：确认 logs、traces、metrics 已经写入 ClickHouse。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/query-clickhouse.sh
```

预期至少看到：

```text
otel_logs
otel_traces
otel_metrics_sum
otel_metrics_histogram
```

其中 `rows` 大于 `0` 说明已经入库。

## 9. 查询 access log 明细

作用：查看每个请求生成的 `info` 级结构化 access log。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/query-access-logs.sh
```

重点字段：

```text
event: http_access
gateway: local
method: GET
path: /
authority: 127.0.0.1:9000
client_ip: 127.0.0.1
x_forwarded_for:
user_agent: curl/8.7.1
downstream_remote_address: 127.0.0.1:xxxxx
route_name: local-test
upstream_host: 127.0.0.1
trace_id: 4bf92f3577b34da6a3ce929d0e0e4736
status_code: 200
request_id: ...
duration_ms: ...
bytes_received:
bytes_sent:
```

## 10. 查询 trace 明细

作用：查看最近写入的 trace span。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/query-traces.sh
```

重点字段：

```text
TraceId
SpanId
SpanName
Duration
http_status_code
request_id
```

## 11. 过滤查看 Collector OTEL 输出

作用：从 Collector 日志中过滤三信号和 access log 相关内容。

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate
scripts/otel-local/check-otel-output.sh
```

重点观察：

```text
otelcol.signal: traces
otelcol.signal: metrics
otelcol.signal: logs
http_access
http access log
```

## 12. 验证 access log

作用：确认每个请求都会生成一条 `info` 级结构化 access log 并写入 ClickHouse。

先发送一次请求：

```bash
curl -v http://127.0.0.1:9000/
```

再查询 ClickHouse：

```bash
scripts/otel-local/query-access-logs.sh
```

期望能看到类似字段：

```text
event: http_access
gateway: local
method: GET
path: /
status_code: 200
request_id: ...
duration_ms: ...
```

## 13. 常见问题

### 只看到 metrics，看不到 logs

作用：检查 logs 配置是否启用。

```bash
cat /tmp/spacegate-otel/config/config.json
```

确认包含：

```json
"logs": {
  "enabled": true,
  "level": "info"
}
```

然后重启 SpaceGate：

```bash
scripts/otel-local/start-spacegate.sh
```

### 请求返回 502 dns error

作用：确认 route 是否指向本地 `127.0.0.1:18080`。

```bash
cat /tmp/spacegate-otel/config/gateway/local/route/root.json
```

确认后端配置是：

```json
"host": {
  "kind": "Host",
  "host": "127.0.0.1"
},
"port": 18080
```

同时确认 Mock AC 已启动：

```bash
curl -v http://127.0.0.1:18080/
```

### Collector 启动后报 ClickHouse 连接失败

作用：确认 ClickHouse 已先启动，并且 Collector 和 ClickHouse 在同一个 Docker network。

```bash
docker ps --filter name=spacegate-clickhouse
docker network inspect spacegate-otel
```

正常情况下，先启动：

```bash
scripts/otel-local/start-clickhouse.sh
```

再启动：

```bash
scripts/otel-local/start-collector.sh
```

### Collector 端口被占用

作用：停止旧 Collector 容器。

```bash
docker rm -f spacegate-otel
```

然后重新启动：

```bash
scripts/otel-local/start-collector.sh
```

### SpaceGate 端口 9000 被占用

作用：查找占用 `9000` 的进程。

```bash
lsof -nP -iTCP:9000 -sTCP:LISTEN
```

如果确认是旧 SpaceGate 进程，可以手动停止对应进程后再启动。

### ClickHouse 查询不到表

作用：确认 Collector 是否已经连接 ClickHouse 并自动建表。

```bash
docker logs spacegate-otel 2>&1 | tail -n 100
```

如果 Collector 启动正常但还没有请求，先发送请求：

```bash
scripts/otel-local/request.sh
```

再查：

```bash
scripts/otel-local/query-clickhouse.sh
```

## 14. 清理本地运行目录

作用：删除本地临时配置。注意这会删除 `/tmp/spacegate-otel`。

```bash
docker rm -f spacegate-otel spacegate-clickhouse
rm -rf /tmp/spacegate-otel
```

重新生成：

```bash
scripts/otel-local/prepare-config.sh
```
