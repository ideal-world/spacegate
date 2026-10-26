# HAI SSE 透传与并发租约设计

## 目标

修复原生 `hai-observe` 将 SSE 全量缓存后才下发的问题，并将 HAI 资产并发限流从可残留的共享计数改为可自动过期的请求租约。满额请求仍立即返回 429。

## 范围

- SSE 响应逐帧原样下传；仅在 EOF 解析 `data: [DONE]` 之前最后一个完整 JSON SSE event 的 `usage`。
- 非 SSE 保持现有完整 JSON 响应解析行为。
- Redis 并发配额使用每请求独立 lease，覆盖成功、超时、取消、客户端断开和进程异常。
- 对齐旧 Wasm 的 model/runtime/MCP endpoint path suffix 拼接，并对照旧 `hai-process-mix` Wasm 插件和 `server` 输出功能迁移审计报告。

不在本次改动中处理请求头清理或历史 502 的诊断日志。

## Endpoint Path 拼接

旧 Wasm 对 `model`、`runtime` 及 MCP 请求会把网关标准路径中资产 ID 之后的 suffix 追加到 `runtime_endpoint`。原生 `hai-dispatch` 恢复相同规则：仅当 suffix 存在时拼接；没有 suffix 时完整保留 `runtime_endpoint`。例如资产 endpoint 为 `https://api.example.com/v1/responses`、请求为 `/api/v1/model/demo` 时保持 `/v1/responses`；请求为 `/api/v1/model/demo/chat/completions` 时发往 `/v1/responses/chat/completions`；MCP 的 `/mcp-services/demo/messages` 同样追加 `/messages`。

## SSE 观测

`HaiObservePlugin` 在响应头为 `text/event-stream` 时创建 `SseObserveBody`。该 body 包装原始 `SgBody`：

1. 每次 `poll_frame` 得到 data frame 都立即返回该 frame。
2. 内部只保留一个有上限的 SSE event tail，并记录最后一个可解析 JSON `data:` event；`[DONE]` 不覆盖记录。
3. 在 EOF、上游 body error 或 body 被 drop 时只执行一次 finalize；EOF 时从记录的最后 JSON event 解析 usage，随后写审计日志和指标。

非 SSE 继续调用 `SgBody::dump()`，以保持 JSON usage 的既有行为。

## 并发租约

每个成功 acquire 的请求获得随机 `lease_id`。Redis 使用资产级 ZSET 保存 member=lease_id、score=到期时间。Lua acquire 脚本会先删除过期 score，再执行 ZCARD 限额检查，最后写入当前 lease；release 使用 ZREM。

已获取的 lease 在活跃请求期间定时 heartbeat 刷新自身 score。正常 EOF、上游超时、future 取消、客户端断连都立即执行 release；release 失败仅记录日志，由 TTL 清理。正常重启会尽力释放；崩溃/强制终止无法保证即时通知 Redis，但 heartbeat 停止后该单个 lease 会在 TTL 内过期，不会受其他请求 heartbeat 影响。

lease TTL 取 `asset.timeout_sec` 加固定容差，未配置时使用 120 秒默认值；heartbeat 周期为 TTL 的三分之一。QPS 秒窗口逻辑保持不变。

## 发布迁移

新 lease ZSET 使用新键名，避免与旧字符串计数类型冲突。切换时必须停止或摘流所有运行旧版本 HAI quota 的网关实例，确认旧在途调用结束后删除受影响资产的旧 `hai:quota:concurrent:<asset_id>` 键，再整体切换新镜像。旧新版本不得长期混跑。

## 验收

- 上游首个 SSE chunk 在 EOF 前到达客户端。
- `[DONE]` 前一个 JSON event 的 usage 只在 EOF 记录一次。
- 非 SSE JSON usage 解析不回归。
- model/runtime/MCP 资产的 endpoint suffix 拼接与旧 Wasm 一致，且完整 endpoint 在没有 suffix 时不被覆盖。
- 并发满额立即返回 429；超时、取消、body drop 和 lease 到期后均可重新 acquire。
- 审计报告逐项说明旧 Wasm/server 的已迁移、行为差异与遗漏。
