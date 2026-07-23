# AI Gateway Queue — 设计与代码差距修复清单

> 对照文档：`ai-gateway-queue-design.md`（桌面版）  
> 审计范围：`spacegate/plugins/wasm/ai-gateway-queue` + `spacegate/binary/ai-gateway-service`  
> 生成日期：2026-05-23

本文档将设计文档与当前实现的差异整理为**可执行的修复项**，按优先级排序。每项包含：差距说明、建议改法、涉及文件、验收标准、依赖关系。

---

## 优先级说明

| 级别 | 含义 | 建议节奏 |
|------|------|----------|
| **P0** | 核心语义错误，影响限流/队列正确性 | 立即修复 |
| **P1** | 设计明确要求的能力缺失或明显性能/可靠性缺口 | 下一迭代 |
| **P2** | 行为/格式与设计有差异，但不阻断主流程 | 按需排期 |
| **P3** | 文档、默认值、观测增强 | 低优先级 |

---

## P0 — 核心语义

### GAP-001：queue/wait 入队前未做令牌桶准入判定

**设计期望**

- 概述：`Gateway → [Rate Limiter] → Redis Stream → Worker → LLM`
- 限流策略表：三种模式在「触发限流时」分别 429 / 202 / 阻塞等待
- abandon 示例：未触发限流 → 正常 LLM 响应；触发限流 → 429

**当前行为**

- `abandon`：Wasm 调 `/v1/ratelimit/check`，通过则直通上游 ✅
- `queue` / `wait`：Wasm **直接**调 `/v1/queue/enqueue` 或 `/v1/queue/enqueue-and-wait`，**不做限流判断**，所有请求全量入队 ❌

**建议改法**

1. **方案 A（推荐，改 Wasm 插件）**  
   - `queue` / `wait` 在入队前先调 `/v1/ratelimit/check`（与 abandon 共用同一接口）  
   - `allowed: true` → `resume_http_request()` 直通上游（配额内直通）  
   - `allowed: false` →  
     - `queue` → 调 enqueue，返回 202  
     - `wait` → 调 enqueue-and-wait，阻塞等待  

2. **方案 B（改 service 入队接口）**  
   - 在 `enqueue_job()` 开头内联令牌桶逻辑；`allowed: true` 时同步调 upstream 并返回 200（wait）或直接 proxy（需 Gateway 配合，改动面更大）

**涉及文件**

- `spacegate/plugins/wasm/ai-gateway-queue/src/lib.rs`（主改）
- `spacegate/plugins/wasm/ai-gateway-queue/README.md`
- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`（若采用方案 B）
- `spacegate/binary/ai-gateway-service/src/app/queue.rs`（若采用方案 B）
- 集成测试 / e2e 脚本

**验收标准**

- [ ] 租户配额内 + `queue` 策略 → **200**，响应来自上游，**不入队**
- [ ] 租户配额内 + `wait` 策略 → **200**，同步上游响应，**不入队**
- [ ] 租户超额 + `queue` → **202** + `X-Job-Id`
- [ ] 租户超额 + `wait` → 入队等待或 **504** 超时
- [ ] `abandon` 行为保持不变
- [ ] `rate_limited_total{policy,tenant}` 在 queue/wait 超额入队时也有计数

**依赖**：无（应最先做）

**备注**：需与设计方确认 queue 示例中「无论是否触发限流都 202」是否作废；若保留该语义，则 queue 模式不做 GAP-001 直通，仅 wait/abandon 对齐。

---

### GAP-002：queue 模式语义与设计文档内部矛盾需定稿

**设计矛盾点**

- 策略对比表：「限流时入队」
- queue 示例：**「立即返回（无论是否触发限流）」** → 202

**当前行为**

- 与 queue 示例一致：所有 queue 请求都 202 入队

**建议改法**

- **产品/架构定稿二选一**，写入设计文档 v2：
  - **模式 Q1（异步优先）**：queue 永远异步入队，不做直通（维持现状）
  - **模式 Q2（配额内直通）**：配额内直通，超额才 202（需 GAP-001）

**涉及文件**

- 设计文档（外部）
- `spacegate/plugins/wasm/ai-gateway-queue/README.md`
- 前端配置手册 / Admin 文案

**验收标准**

- [ ] 设计文档消除内部矛盾
- [ ] README、前端说明与定稿一致
- [ ] 测试用例覆盖定稿语义

**依赖**：阻塞 GAP-001 的实现细节

---

### GAP-003：多租户配额叠加无全局容量保护

**设计期望**

- 设计强调租户隔离，未写全局上限；但生产上多租户「各自配额内」叠加仍可能打满上游

**当前行为**

- 仅 per-tenant 令牌桶；无 cluster 级总 RPS / 总并发 Semaphore
- `abandon` 直通不受 `worker_concurrency` 约束

**建议改法**

1. 增加 **全局令牌桶**（Redis key 如 `ai:global:ratelimit:tokens`），在 `/v1/ratelimit/check` 中 **先扣全局、再扣租户**
2. 增加 **upstream 并发 Semaphore**（`AI_UPSTREAM_MAX_INFLIGHT`），abandon 直通与 Worker 共享
3. `/metrics` 暴露 `global_rate_limited_total`、`upstream_inflight`

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`
- `spacegate/binary/ai-gateway-service/src/app/types.rs`（Lua 或新函数）
- `spacegate/binary/ai-gateway-service/src/app/config.rs`
- `spacegate/binary/ai-gateway-service/config/ai-gateway-service.example.toml`
- `spacegate/binary/ai-gateway-service/src/app/queue.rs`（Worker 侧 acquire permit）

**验收标准**

- [ ] 100 个租户各在配额内，全局上限触发后后续请求按策略 429/入队/等待
- [ ] 指标可观测全局拒绝次数
- [ ] 配置可独立调整全局 RPS 与 upstream inflight

**依赖**：建议在 GAP-001 之后

---

## P1 — 性能与可靠性

### GAP-004：S3 multipart 上传与 XADD 顺序执行，非设计所述并发

**设计期望**

> 入队（S3 卸载）：S3 PutObject 与 XADD 并发执行，瓶颈在 S3

**当前行为**

- `store_body()` 完整完成后才 `XADD`

**建议改法**

- 小 refactor：`store_body` 返回 `(BodyLocation, future)` 或在超阈值时：
  1. 先 `XADD` 占位 entry（status=uploading）或
  2. 并行：`tokio::join!(multipart_upload, prepare_metadata)`，最后 XADD
- 最小改动：XADD 只写 ref/metadata，body 上传异步完成后更新 entry 或 Worker 按 ref 拉取（Worker 已支持 ref）

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/queue.rs`
- `spacegate/binary/ai-gateway-service/src/app/object_store.rs`

**验收标准**

- [ ] 大 body 场景 enqueue P99 不因「上传完成 + XADD 串行」线性叠加
- [ ] 上传失败时 entry 不处于不可消费状态（abort + DLQ 或重试）

**依赖**：无

---

### GAP-005：wait 模式每请求新建 SubscriberClient，未实现连接复用

**设计期望**

> 1000 个 wait 并发共享同一物理连接（fred 多路复用订阅）

**当前行为**

- 每次 `enqueue_and_wait` 调用 `build_subscriber_client()` 新建连接

**建议改法**

- 在 `AppState` 中维护 **共享 SubscriberClient 池** 或单例 multiplexer
- 按 `result:{job_id}` channel 注册/oneshot 等待，避免 per-request 连接
- 注意：fred API 下订阅与命令连接分离的要求仍满足

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`
- `spacegate/binary/ai-gateway-service/src/app/runtime.rs`（AppState 初始化）
- `spacegate/binary/ai-gateway-service/src/app/util.rs`
- 新增 `wait_subscriber.rs`（可选）

**验收标准**

- [ ] 100 并发 wait 时 Redis 连接数不随请求线性增长
- [ ] 竞态保险（subscribe 后 get result）仍正确
- [ ] 超时后 subscriber 无泄漏

**依赖**：无

---

### GAP-006：Worker XREADGROUP 读 5 条但串行处理

**设计期望**

> 每次 XREADGROUP 取 5 条，**批量并发处理**

**当前行为**

- `read_worker_stream` 循环内逐条 `process_stream_entry`（串行）

**建议改法**

- 对同一 batch 用 `FuturesUnordered` / `tokio::spawn` 并发处理
- 仍受 `worker_concurrency` 或独立 `worker_inflight` Semaphore 约束

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/queue.rs`

**验收标准**

- [ ] 队列积压时 Worker 吞吐随 concurrency 提升
- [ ] job lease 机制下无重复执行
- [ ] upstream inflight（GAP-003）不被突破

**依赖**：建议与 GAP-003 一并设计

---

### GAP-007：未配置 object_store 时大 body 仍 inline 进 Redis

**设计期望**

- 超 128KB 应 offload 到 S3；Redis entry 只存 ref

**当前行为**

- 仅当 `object_store.endpoint` 配置存在时才 multipart；否则 >128KB 仍 base64 写入 Stream

**建议改法**

- 启动时：若 `inline_threshold` 较小但未配 object_store，**warn 或 fail_fast**（生产配置）
- 或：超阈值且无 S3 时拒绝入队并返回 **413 Payload Too Large**

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/object_store.rs`
- `spacegate/binary/ai-gateway-service/src/app/config.rs`（校验）
- `spacegate/binary/ai-gateway-service/config/ai-gateway-service.example.toml`

**验收标准**

- [ ] 生产配置下 >128KB 请求不会把大 payload 塞进 Redis
- [ ] 本地无 MinIO 时行为明确（拒绝或强制配 endpoint）

**依赖**：无

---

## P2 — 协议与行为对齐

### GAP-008：`rate_limited_total{policy,tenant}` 仅 abandon 路径计数

**设计期望**

- 监控：各策略触发限流次数

**当前行为**

- 仅在 `check_rate_limit` handler 内 increment；queue/wait 超额入队不计数

**建议改法**

- GAP-001 完成后，在「超额转 queue/wait 分支」同样 `inc_labeled`
- 或抽取 `record_rate_limited(policy, tenant)` 共用

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`
- `spacegate/plugins/wasm/ai-gateway-queue/src/lib.rs`（若 Wasm 侧判定）

**验收标准**

- [ ] queue/wait 因配额拒绝而入队时，`rate_limited_total{policy="queue",tenant="..."}` 递增

**依赖**：GAP-001

---

### GAP-009：回调 JSON 与设计示例字段不完全一致

**设计期望**

```json
{
  "job_id": "...",
  "status": "completed",
  "result": { ...LLM 响应... },
  "completed_at": "2024-01-01T12:00:01Z"
}
```

**当前行为**

- 额外字段：`http_status`、`headers`、`body_base64`、`completed_at_ms`、`error`

**建议改法**

- **方案 A**：文档化当前 schema 为正式 API（推荐，向后兼容）
- **方案 B**：增加 `callback_format=v1|v2` 或 Accept 头切换精简格式

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/callback.rs`
- `spacegate/binary/ai-gateway-service/README.md`

**验收标准**

- [ ] API 文档与实现一致
- [ ] 若有 v1 精简格式，集成测试覆盖

**依赖**：无

---

### GAP-010：job_id 格式与设计示例不一致

**设计期望**

- 示例：`01J8XYZABC`（类 ULID）

**当前行为**

- `{timestamp_hex}{counter_hex}`

**建议改法**

- 改用 ULID / UUID v7；或保留现状并更新设计文档

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/util.rs`（`new_job_id`）

**验收标准**

- [ ] job_id 全局唯一、可排序（若用 ULID）
- [ ] 旧 job 查询不受影响（无需迁移）

**依赖**：无

---

### GAP-011：`X-RateLimit-Policy` 可通过配置绕过

**设计期望**

- 请求头表格：Policy **必填**

**当前行为**

- `require_policy=false` 且无 default 时 Wasm `Action::Continue` 完全 bypass

**建议改法**

- 生产 preset：`require_policy=true` 且文档标注勿关闭
- 或移除 bypass 路径，仅允许 `default_policy` fallback

**涉及文件**

- `spacegate/plugins/wasm/ai-gateway-queue/src/lib.rs`
- Admin 前端默认值 / 校验

**验收标准**

- [ ] 生产配置无法意外 bypass 插件
- [ ] 缺少 policy 一律 400

**依赖**：无

---

### GAP-012：HTTPS 回调要求可关闭

**设计期望**

- `X-Callback-URL` 需 HTTPS

**当前行为**

- `require_https_callback` 默认 true，可 env 关闭

**建议改法**

- 生产 profile 强制 HTTPS；dev profile 允许 HTTP
- 配置校验：非 dev 且 `require_https=false` 启动 warning/error

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/config.rs`
- `spacegate/binary/ai-gateway-service/src/app/queue.rs`（`validate_callback_url`）

**验收标准**

- [ ] 生产启动检查通过
- [ ] 本地 `AI_REQUIRE_HTTPS_CALLBACK=false` 仍可用

**依赖**：无

---

### GAP-013：令牌桶粒度设计写「仅 Tenant」，实现为 tenant+model+path

**设计期望**

- 限流粒度按 `X-Tenant-Id` 隔离

**当前行为**

- Redis key：`ai:ratelimit:{tenant}:{model}:{path}` + Admin 多维规则

**建议改法**

- **推荐**：更新设计文档 v2，声明更细粒度为 intentional enhancement
- 若需严格 tenant-only：增加配置 `rate_limit_granularity=tenant|tenant_model_path`

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`
- 设计文档

**验收标准**

- [ ] 文档与实现一致
- [ ] 可选配置切换粒度（若做）

**依赖**：无

---

## P3 — 默认配置、观测与文档

### GAP-014：优先级 Stream 默认关闭

**设计期望**

- 扩展：多 Stream 优先级（high/low）

**当前行为**

- `enable_priority_streams` 默认 `false`

**建议改法**

- 生产 example toml 设为 `true`
- 或 Wasm `plugin_config.priority.enabled` 与 service 配置联动文档化

**涉及文件**

- `spacegate/binary/ai-gateway-service/config/ai-gateway-service.example.toml`
- `spacegate/plugins/wasm/ai-gateway-queue/README.md`

**验收标准**

- [ ] 启用后 high/low stream 有深度指标
- [ ] Worker 按权重消费

**依赖**：无

---

### GAP-015：监控指标命名与设计略有差异

**设计期望**

- `enqueue_latency_ms{policy,size_bucket}` 等

**当前行为**

- Prometheus 文本 + `_bucket{le=...}` histogram 风格；部分为 counter

**建议改法**

- 导出与设计对齐的 gauge/histogram（OpenMetrics）
- 或更新设计文档指标名

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/handlers.rs`（`/metrics`）
- `spacegate/binary/ai-gateway-service/src/app/metrics.rs`

**验收标准**

- [ ] Grafana 面板可按设计指标名查询
- [ ] `queue_depth > 1000`、`pel_size > 100` 告警规则可配置

**依赖**：无

---

### GAP-016：Redis 版本未校验

**设计期望**

- Redis 7+（Stream、Pub/Sub）

**当前行为**

- 运行时未检查版本

**建议改法**

- 启动时 `INFO server` 检查 major >= 7，否则 warn/error

**涉及文件**

- `spacegate/binary/ai-gateway-service/src/app/runtime.rs`

**验收标准**

- [ ] Redis 6 启动给出明确错误信息

**依赖**：无

---

### GAP-017：Wasm 层 README / 插件文档与实现对齐

**当前缺口**

- README 仍描述「超额入队」，未说明 queue/wait 当前全量入队
- 未说明 abandon 与 queue/wait 限流路径差异

**建议改法**

- GAP-001 / GAP-002 定稿后一次性更新：
  - `spacegate/plugins/wasm/ai-gateway-queue/README.md`
  - Admin 内嵌 readme API 同源
  - `spacegate/docs/` 前端配置手册（若有）

**验收标准**

- [ ] 文档描述与代码行为一致
- [ ] curl 示例可 copy 运行通过

**依赖**：GAP-001、GAP-002

---

## 建议实施顺序（Roadmap）

```text
Phase 0 — 定稿（1-2 天）
  GAP-002  queue 模式语义定稿
  GAP-013  限流粒度文档对齐

Phase 1 — 核心正确性（1-2 周）
  GAP-001  queue/wait 入队前令牌桶（或确认 Q1 不做）
  GAP-008  限流指标补全
  GAP-003  全局容量保护
  GAP-011  生产禁止 bypass policy

Phase 2 — 性能与可靠性（1-2 周）
  GAP-005  wait Subscriber 连接复用
  GAP-006  Worker 批量并发
  GAP-004  S3 + XADD 并发
  GAP-007  大 body 无 S3 保护

Phase 3 — 对齐与 polish（按需）
  GAP-009 ~ GAP-017
```

---

## 测试清单（每项修复必跑）

| 场景 | 命令/用例 |
|------|-----------|
| abandon 配额内 | Policy=abandon，RPS 内 → 200 来自 upstream |
| abandon 超额 | → 429 + Retry-After |
| queue 配额内 | 定稿 Q2：200 直通；Q1：202 |
| queue 超额 | → 202 + callback |
| wait 配额内 | 定稿 Q2：200 同步；否则入队等待 |
| wait 超额/超时 | → 504 + poll_url，job 仍完成 |
| 大 body offload | >128KB + MinIO → Redis 仅 ref |
| Worker 崩溃 | kill worker → XAUTOCLAIM 重认领 |
| 回调失败 | 不可达 URL → retry stream → DLQ |
| 多租户叠加 | 触发 GAP-003 全局上限 |

现有脚本参考：

- `spacegate/binary/ai-gateway-service` 下 unit tests
- `tests/queue-object-store-e2e.sh`（若有）

---

## 变更影响矩阵

| GAP | Wasm 插件 | ai-gateway-service | Admin 前端 | 破坏性 |
|-----|-----------|-------------------|------------|--------|
| 001 | ✅ | 可选 | 文案 | **高**（queue 从全 202 变为配额内 200） |
| 002 | — | — | 文案 | 产品决策 |
| 003 | — | ✅ | 可选配额 UI | 中 |
| 004 | — | ✅ | — | 低 |
| 005 | — | ✅ | — | 低 |
| 006 | — | ✅ | — | 低 |
| 007 | — | ✅ | — | 中（大 body 可能从能入队变 413） |
| 008 | 可选 | ✅ | — | 低 |
| 011 | ✅ | — | ✅ | 中 |
| 014 | 配置 | ✅ | ✅ | 低 |

---

## 开放问题（实施前需确认）

1. **queue 模式**：永远 202（Q1）还是配额内直通（Q2）？
2. **wait 模式**：配额内是否应同步直通上游，还是始终走队列（便于统一 observability）？
3. **全局容量**：是否需要独立配置项暴露给 Admin，还是仅 ops 环境变量？
4. **GAP-001 方案 A vs B**：限流判定放在 Wasm 还是 service 入队接口内？
5. **job_id 是否改为 ULID**：有无外部系统已依赖当前 hex 格式？

---

## 修订记录

| 日期 | 说明 |
|------|------|
| 2026-05-23 | 初版：基于设计文档 vs 代码审计生成 |
| 2026-05-24 | **DOC-01/02 定稿**：遵循概述 `Gateway → [Rate Limiter] → …` 与策略表「限流时」语义。三种策略均先过令牌桶；**配额内直通上游**（`resume_http_request`）；超额时 abandon→429、queue→202 入队、wait→入队并阻塞等待。queue 示例「无论是否限流都 202」以策略表为准作废。 |
| 2026-05-24 | **全量差距项实施完成**：G-01~G-20、A/Q/W 分项及 DOC 定稿均已落地；`cargo test -p ai-gateway-service` 14/14 通过。详见各模块 commit 与 README 更新。 |

---

## DOC-01 / DOC-02 定稿结论（2026-05-24）

| 策略 | 配额内（allowed） | 超额（rate limited） |
|------|------------------|---------------------|
| abandon | 直通上游 | 429 |
| queue | 直通上游 | 202 异步入队 |
| wait | 直通上游 | 入队 + Pub/Sub 阻塞等待 |
