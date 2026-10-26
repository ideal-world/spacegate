# HAI SSE 透传、并发租约与迁移审计 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 保证 HAI SSE 首包透传、在 EOF 解析最终 usage，并用可过期 Redis 请求租约防止异常并发计数残留。

**Architecture:** `hai-observe` 用 response-body observer 取代 SSE `dump`，只保存末尾完整 SSE JSON event。`hai-quota` 将字符串并发计数改为每请求 ZSET lease，Lua 脚本原子 acquire，body 生命周期和取消路径 release，heartbeat/TTL 处理进程异常。迁移审计只读对照旧 Wasm/server 和原生插件。

**Tech Stack:** Rust、Hyper Body、Tokio、Redis Lua/ZSET、SpaceGate plugin。

## Global Constraints

- 不自动创建 git commit。
- 先运行每个新增测试并确认 RED，再写生产实现。
- model/runtime/MCP endpoint path suffix 对齐旧 Wasm；不处理请求头清理和原始 502 日志。
- 满额请求立即返回现有 429，不实现排队。
- 旧新 quota 版本不能长期混跑；发布前按设计文档清理旧键。

---

### Task 1: 建立 SSE 首包与末尾 usage 的回归测试

**Files:**
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/observe.rs`

**Interfaces:**
- Consumes: `HaiObservePlugin::call(Request<SgBody>, Inner)`。
- Produces: 测试专用可控上游 body；断言客户端在 upstream EOF 前获得首帧，EOF 时审计状态解析最后 JSON event 的 usage。

- [ ] **Step 1: 写失败测试**

在 `observe.rs` 的 `#[cfg(test)]` 模块添加一个 channel body：先发送 `data: {\"usage\":...}\n\n`，等待测试通知后再发送 `data: [DONE]\n\n` 并 EOF。调用 observe 包装后的服务后，在允许上游 EOF 前轮询下游 body，断言已经取得第一帧；EOF 后断言保存的 usage 等于 JSON event，而不是 `[DONE]`。

- [ ] **Step 2: 运行 RED**

Run: `cargo test -p hai-hub-spacegate-plugins observe_sse_first_frame -- --nocapture`

Expected: FAIL，因为当前 `body.dump()` 在上游 EOF 前不返回 response。

- [ ] **Step 3: 最小实现**

新增私有 `SseObserveBody`，实现 `hyper::body::Body`。data frame 原样返回；使用有限 tail 按 SSE 空行边界记录最后一个可解析 JSON `data:` event；在 EOF 单次 finalize audit/metrics。`HaiObservePlugin::call` 仅在 `content_type_is_sse` 时使用该 body，其他响应保留 dump 分支。

- [ ] **Step 4: 运行 GREEN**

Run: `cargo test -p hai-hub-spacegate-plugins observe_sse_first_frame -- --nocapture`

Expected: PASS，首帧先于 EOF 到达，usage 在 EOF 后正确记录。

### Task 2: 将并发计数替换为可过期租约

**Files:**
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/quota.rs`
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/util.rs`

**Interfaces:**
- Consumes: `AssetRecord.max_concurrent`、`AssetRecord.timeout_sec`、Gateway Redis client。
- Produces: `QuotaLease`，提供 acquire、heartbeat、release；ZSET key `hai:quota:lease:<asset_id>`。

- [ ] **Step 1: 写失败测试**

使用现有 Redis test-container helper 新建 quota 集成测试：第一个 request acquire 后第二个同资产 request 在 limit=1 时返回 429；显式取消第一个 future 或 drop response body 后，第三个 request 能 acquire；另一个测试插入过期 lease 后断言下一次 acquire 清理并通过。

- [ ] **Step 2: 运行 RED**

Run: `cargo test -p hai-hub-spacegate-plugins quota_lease -- --nocapture`

Expected: FAIL，因为当前实现只使用 `INCR/DECR` 字符串计数，取消/过期 lease 无法清理。

- [ ] **Step 3: 最小实现**

将 acquire Lua 改为 ZREMRANGEBYSCORE、ZCARD、ZADD 的原子操作；随机生成 `lease_id`。`QuotaLease` 在成功响应的 body EOF/drop、dispatch timeout future 被取消时调用幂等 `ZREM`，并启动 heartbeat 刷新本 lease。保留 QPS window Lua 行为和现有 429 JSON error。

- [ ] **Step 4: 运行 GREEN**

Run: `cargo test -p hai-hub-spacegate-plugins quota_lease -- --nocapture`

Expected: PASS，满额 429、取消释放、过期清理均通过。

### Task 3: 验证非 SSE 与全插件回归

**Files:**
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/observe.rs`
- Test: 现有 `observe.rs` unit tests。

**Interfaces:**
- Consumes: 普通 JSON 上游 response。
- Produces: 仍使用 `dump` 的 JSON usage 回归断言。

- [ ] **Step 1: 写失败测试**

添加普通 JSON response case，断言 `usage`、状态和 response body 均保持当前结果，避免 SSE 分支误用于普通响应。

- [ ] **Step 2: 运行 RED**

Run: `cargo test -p hai-hub-spacegate-plugins observe_non_sse_usage -- --nocapture`

Expected: 在引入 SSE 分支但未保留 JSON dump 时 FAIL。

- [ ] **Step 3: 保留最小 JSON 分支**

只对 `text/event-stream` 走 observer；其他 content type 维持当前 `dump`/JSON usage 逻辑。

- [ ] **Step 4: 运行 GREEN 与包级回归**

Run: `cargo test -p hai-hub-spacegate-plugins -- --nocapture`

Expected: PASS。

### Task 4: 恢复 model/runtime/MCP endpoint path suffix 拼接

**Files:**
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/dispatch.rs`
- Modify: `../hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/util.rs`

**Interfaces:**
- Consumes: 标准请求路径 `/api/v1/{asset_type}/{asset_id}/{suffix}`、资产 `asset_type` 与 `runtime_endpoint`。
- Produces: model/runtime/MCP 有 suffix 时使用 `runtime_endpoint + suffix` 的完整上游 URI；无 suffix 时保持 `runtime_endpoint` 原值。

- [ ] **Step 1: 写失败测试**

在 `dispatch.rs` 添加两个测试：model 资产、endpoint 为 `https://api.example.com/v1`、请求路径为 `/api/v1/model/demo/chat/completions` 时 capture 的 URI 必须为 `https://api.example.com/v1/chat/completions`；runtime 资产走相同规则。另加 MCP `/mcp-services/demo/messages` 与无 suffix case，endpoint 分别追加 `/messages` 和保持原样。

- [ ] **Step 2: 运行 RED**

Run: `cargo test -p hai-hub-spacegate-plugins runtime_endpoint_path_suffix -- --nocapture`

Expected: FAIL，因为当前 dispatch 直接使用 `runtime_endpoint`，没有追加 asset 后的请求路径。

- [ ] **Step 3: 最小实现**

在 `util.rs` 新增与旧 Wasm 同语义的路径 suffix 提取和 URL 拼接 helper；dispatch 对 `model`、`runtime` 与 MCP 请求调用该 helper，再继续现有密钥注入、Host 改写和 deferred URI 恢复。

- [ ] **Step 4: 运行 GREEN**

Run: `cargo test -p hai-hub-spacegate-plugins runtime_endpoint_path_suffix -- --nocapture`

Expected: PASS，model/runtime/MCP suffix 拼接和完整 endpoint 保留均通过。

### Task 5: 完成 Wasm/server 到原生插件迁移审计

**Files:**
- Create: `spacegate/docs/hai-process-mix-native-migration-audit.md`

**Interfaces:**
- Consumes: `hai-process-mix/plugins/mix/src/http_filter.rs`、`hai-process-mix/server/src/{handler,headers,protocol}.rs` 与 `hai-hub-spacegate-plugins/src/plugin/hai/*.rs`。
- Produces: 按认证、资产状态、限流、密钥注入、路径/方法、Host/控制头、超时、SSE、usage、输出保护、审计逐项比对的结论和补救优先级。

- [ ] **Step 1: 建立功能矩阵**

列出旧实现和原生实现的代码入口、是否等价、差异及风险；特别标记 SSE dump 回归、租约语义变更、server 控制头剥离、endpoint suffix 规则、output guard 和审计上报。

- [ ] **Step 2: 以现有测试/源码逐项核验**

每个“已迁移”条目链接原实现和原生实现；每个“差异/遗漏”条目列出是否属于本次范围。

- [ ] **Step 3: 完成审计报告**

Run: `rg -n 'TBD|TODO|待确认' spacegate/docs/hai-process-mix-native-migration-audit.md`

Expected: 无输出；报告不包含未定结论。

### Task 6: 最终验证和发布交接

**Files:**
- Modify: `spacegate/docs/superpowers/specs/2026-07-28-hai-sse-quota-design.md`（仅在实现与设计不一致时更新）。

- [ ] **Step 1: 运行格式化检查**

Run: `cargo fmt --check --manifest-path ../hai-hub/Cargo.toml`

Expected: exit 0。

- [ ] **Step 2: 运行目标测试与 workspace build**

Run: `cargo test -p hai-hub-spacegate-plugins --manifest-path ../hai-hub/Cargo.toml && cargo build -p hai-hub-spacegate --manifest-path ../hai-hub/Cargo.toml`

Expected: exit 0。

- [ ] **Step 3: 复核变更范围**

Run: `git -C ../hai-hub diff --check && git -C ../hai-hub status --short && git -C spacegate diff --check`

Expected: 无 whitespace error；仅包含本计划的源文件、测试和审计文档。不得自动 commit。
