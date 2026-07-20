# AI 网关排队限流插件 — 管理界面配置指南

> 历史流程：本文要求手工创建并绑定插件实例，已不适用于当前 `ai-gateway-queue` 系统内置插件模型；仅保留用于追溯旧实现。

本文说明如何在 **SpaceGate Admin 管理界面** 中配置 **AI 请求队列网关**（`ai-gateway-queue` Wasm 插件），包括插件实例创建、网关/路由挂载、租户配额，以及客户端请求头约定。

相关文档：

- 插件行为与 API：[`plugins/wasm/ai-gateway-queue/README.md`](../../../plugins/wasm/ai-gateway-queue/README.md)
- 编译与 K8s 部署：[`deploy/README.md`](../../../deploy/README.md)
- 当前测试用例：[`test-spec.md`](../../ai-gateway/test-spec.md)

---

## 1. 配置全景

管理界面上的配置分 **三层**，需按顺序完成：

```text
┌─────────────────────────────────────────────────────────────┐
│ ① 插件实例（插件页 → AI 请求队列网关）                        │
│    写入 plugin/wasm.ai-gateway-queue.json                    │
│    含 Wasm URL、后端地址、plugin_config 等                   │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│ ② 挂载引用（网关页 或 路由页 → 插件列表）                     │
│    仅引用 { code: wasm, name: ai-gateway-queue }             │
│    ⚠ 只选一层挂载，勿 Gateway + Route 重复                    │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│ ③ 租户配额（ai-gateway-service Admin API）                   │
│    UI「队列配额」Tab 当前为占位；可用 API 或 curl 配置          │
└─────────────────────────────────────────────────────────────┘
```

| 层级 | 管理界面入口 | 落盘 / 存储 |
|------|-------------|-------------|
| 插件实例 | **插件** → Tab **AI** → **AI 请求队列网关** | `plugin/wasm.ai-gateway-queue.json` |
| 挂载引用 | **网关** 或 **路由** → 插件列表 | `gateway/{name}/config.json` 或 `route/{route}.json` |
| 租户配额 | API（见 §6） | Redis |

---

## 2. 前置条件

### 2.1 依赖服务

| 服务 | 默认端口 | 说明 |
|------|---------|------|
| **spacegate-admin-server** | 9992（开发）/ 9080（Docker 管理端） | 读写 SpaceGate 配置 |
| **spacegate-admin-fe** | 4000 | Vue 管理界面 |
| **SpaceGate 网关** | 9993 | 加载 Wasm 并转发流量 |
| **ai-gateway-service** | 18080 | 限流 / 入队 / Worker 后端 |
| **Redis 7+** | 6379 | 令牌桶与队列 |
| **上游 LLM** | 9000（示例） | HTTPRoute 后端 |

### 2.2 启动管理界面（本地开发）

```bash
# 终端 1：Admin 后端（文件配置模式示例）
cd spacegate
cargo run -p spacegate-admin-server -- -c file:.docker/ai-gateway-demo

# 终端 2：Admin 前端
cd spacegate-admin-fe
npm install
npm run dev
# 浏览器打开 http://localhost:4000
```

Docker 环境可直接访问 **`http://localhost:9080`**（`ai-gateway-web` 容器）。

### 2.3 配置 ai-gateway-service 地址（重要）

插件 Drawer 中的 **Schema 表单**、**文档 Tab** 以及未来的 **租户配额** 均通过 `ai-gateway-service` 的 Admin API 拉取：

```bash
# spacegate-admin-fe/.env.local（或构建时环境变量）
VITE_AI_GATEWAY_BASE_URL=http://127.0.0.1:18080
```

| 是否配置 | 效果 |
|---------|------|
| **已配置** | Schema / Readme 正常加载；租户配额 API 可用 |
| **未配置** | 请求打到前端 `:4000`，Schema 加载失败，表单为空 |

SpaceGate 配置 API（保存插件、网关、路由）走 `/api` 代理到 admin-server，**与上述变量无关**。

本地 Vite 代理（`vite.config.ts`）：

```text
/api/*  →  http://localhost:9992/*
```

---

## 3. 界面导航

### 3.1 选择网关

顶部 **SelectGateway** 下拉框选择目标网关（如 `ai-demo`）。后续菜单跳转会自动带上 `?gatewayName=ai-demo`。

### 3.2 左侧菜单

| 菜单 | 路径 | 与本插件相关用途 |
|------|------|-----------------|
| **网关** | `/gateway` | 网关级插件挂载、监听器 |
| **路由** | `/route` | 路由规则、后端、规则级插件 |
| **插件** | `/plugins` | **创建 / 编辑 AI 请求队列网关实例** |
| **实例** | `/instance` | SpaceGate 进程在线状态（与插件配置无关） |

---

## 4. 分步配置

### 步骤 1：创建插件实例

1. 进入 **插件** 页
2. 切换到 Tab **「AI」**
3. 找到卡片 **「AI 请求队列网关」**
4. 点击 **「配置」**，打开 **AI 请求队列网关** Drawer
5. 填写 **基础接入** 与 **基础配置**（见 §5）
6. 点击 **保存**

保存后 admin-server 写入：

```text
plugin/wasm.ai-gateway-queue.json
```

首次保存调用 `POST /config/plugin`；再次编辑调用 `PUT /config/plugin`。

卡片上会显示 **「已部署」** 标签。

### 步骤 2：挂载到网关或路由

插件实例创建后，还需在 **网关** 或 **路由** 中引用，流量才会经过 Wasm。

#### 方式 A：网关级挂载（推荐）

1. 进入 **网关** 页
2. 编辑目标网关（如 `ai-demo`）
3. 找到 **插件** 字段（PluginListForm）
4. 点击 **添加插件**
5. 选择：
   - **Code**：`wasm`
   - **Kind**：`named`
   - **Name**：`ai-gateway-queue`
6. 保存网关配置

等价 JSON 片段：

```json
{
  "plugins": [
    {
      "code": "wasm",
      "kind": "named",
      "name": "ai-gateway-queue"
    }
  ]
}
```

#### 方式 B：路由级挂载

1. 进入 **路由** 页
2. 编辑目标路由（如 `ai`）下的某条 **规则**
3. 在规则 **插件** 列表中添加同样的引用
4. 配置 **后端** 指向 LLM 上游
5. 保存

等价 JSON 片段（规则内）：

```json
{
  "matches": [{ "path": { "kind": "Prefix", "value": "/v1/" } }],
  "plugins": [
    { "code": "wasm", "kind": "named", "name": "ai-gateway-queue" }
  ],
  "backends": [{ "host": { "kind": "Host", "host": "127.0.0.1" }, "port": 9000, "weight": 1 }]
}
```

> **⚠ 切勿重复挂载**  
> 若 Gateway 与 Route **同时** 引用 `ai-gateway-queue`，每个请求会执行 **两次** 插件逻辑，导致 **双倍扣 token / 双倍限流**。  
> 生产环境请 **只选一层**；`resource/ai-gateway-demo` 示例为演示方便两处都挂了，本地验证时注意这一点。

### 步骤 3：配置路由与后端

在 **路由** 页确保：

- 路径匹配 AI API（如 `/v1/` Prefix）
- **后端** 指向真实 LLM 服务地址与端口
- 优先级（priority）按需设置

### 步骤 4：配置租户配额（可选）

按租户 / 模型 / 路径 / 策略设置差异化令牌桶，见 **§6**。当前 Drawer 内 **「队列配额」Tab 为占位**，需通过 API 配置。

### 步骤 5：验证

```bash
# 经网关（插件生效）
curl -i http://127.0.0.1:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: demo' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"hello"}'

# 直连后端健康检查
curl http://127.0.0.1:18080/healthz
```

期望：配额内 `200`；缺 Policy 且 `require=true` 时 `400`；超额 abandon `429`。

---

## 5. Drawer 字段说明

打开 **插件 → AI → AI 请求队列网关 → 配置** 后，Drawer 含四个 Tab。

### 5.1 Tab「基础配置」

#### 基础接入（Wasm 宿主层 → `spec` 顶层）

| 界面字段 | 配置键 | 默认值 | 说明 |
|---------|--------|--------|------|
| Wasm URL | `url` | 空 | Wasm 制品地址。支持 `file://`、`http(s)://`、`oci://` |
| 插件名称 | `plugin_name` | `ai-gateway-queue` | 建议保持不变 |
| 失败策略 | `fail_strategy` | `fail_close` | `fail_close`：插件异常时拒绝请求；`fail_open`：放行 |
| 队列后端地址 | `clusters["ai-gateway-service"]` | `http://127.0.0.1:18080` | ai-gateway-service 的 HTTP 地址 |
| 普通 VM 池大小 | `vm_pool_size` | `4` | 处理 abandon / queue 短请求的 Wasm 实例数，≥1 |
| Wait VM 池大小 | `wait_vm_pool_size` | `4` | wait 长连接专用池；不用 wait 可设 `0` |

**Wasm URL 示例：**

| 环境 | 示例值 |
|------|--------|
| 本地 Cargo | `file:///path/to/spacegate/plugins/wasm/target/wasm32-wasip1/release/spacegate_plugin_ai_gateway_queue.wasm` |
| Docker 挂载 | `file:///etc/spacegate/plugins/spacegate_plugin_ai_gateway_queue.wasm` |
| K8s HTTP 分发 | `http://ai-gateway-wasm/spacegate_plugin_ai_gateway_queue.wasm` |
| OCI 制品 | `oci://ghcr.io/your-org/ai-gateway-queue:v1.0.0` |

**界面未暴露、保存时会保留的字段**（来自已有配置文件）：

- `validate_on_create`、`plugin_root_id`、`plugin_vm_id`
- `limits`：`max_memory_pages`、`fuel_per_call`、`epoch_timeout_millis`、`max_body_bytes`、`max_pending_calls`

#### 基础配置 Schema 表单（→ `spec.plugin_config`）

表单字段由 `ai-gateway-service` 动态提供：`GET /v1/admin/plugins/ai-gateway-queue/schema`。

##### service — 队列后端接入

| 字段 | 默认 | 说明 |
|------|------|------|
| `cluster` | `ai-gateway-service` | SpaceGate cluster 名，须与 `clusters` 键一致 |
| `authority` | `ai-gateway-service` | HTTP 调用的 `:authority` |
| `timeout_ms` | `65000` | 调用后端超时；使用 wait 模式建议 ≥60000 |

##### paths — 后端 API 路径

| 字段 | 默认 |
|------|------|
| `rate_limit` | `/v1/ratelimit/check` |
| `enqueue` | `/v1/queue/enqueue` |
| `wait` | `/v1/queue/enqueue-and-wait` |

一般保持默认即可，除非后端改了路由前缀。

##### headers — 客户端请求头映射

| 字段 | 默认 HTTP 头 | 用途 |
|------|-------------|------|
| `policy` | `X-RateLimit-Policy` | 队列策略 |
| `tenant` | `X-Tenant-Id` | 租户标识 |
| `model` | `X-Model` | 模型名（优先级路由） |
| `priority` | `X-Queue-Priority` | 显式优先级 |

HTTP 头名大小写不敏感；配置中通常写小写。

##### policies — 策略校验

| 字段 | 默认 | 说明 |
|------|------|------|
| `require` | `true` | 为 `true` 时，缺少 Policy 头 → **400** |
| `default` | 空 | `require=false` 时使用的默认策略：`abandon` / `queue` / `wait` |

##### priority — 多优先级队列

| 字段 | 默认 | 说明 |
|------|------|------|
| `enabled` | `true` | 关闭后所有请求走 `default` 优先级 |
| `default` | `normal` | `high` / `normal` / `low` |
| `high_models` / `low_models` | `[]` | 模型名精确匹配 |
| `high_tenants` / `low_tenants` | `[]` | 租户 ID 列表 |

> **扁平 vs 嵌套格式**  
> 部分示例文件（如 `resource/ai-gateway-demo`）使用扁平键（`service_cluster`、`require_policy`）。  
> 管理界面 SchemaForm 使用 **嵌套 JSON**。Wasm 运行时两种格式均兼容；若从文件导入后表单显示异常，可在 Drawer 中重新保存一次以统一格式。

### 5.2 Tab「队列配额」

当前版本显示占位说明：**租户差异化限流 UI 尚未接入 Drawer**。

V1 行为说明（与界面提示一致）：

- **全局限流**在 `ai-gateway-service` 配置，非 Drawer 字段
- 环境变量：`AI_RATE_LIMIT_RPS`、`AI_RATE_LIMIT_BURST`、`AI_RATE_LIMIT_COST`
- 或 TOML `[rate_limit]` 段

租户级配额请使用 **§6 API**。

### 5.3 Tab「文档」

从 `GET /v1/admin/plugins/ai-gateway-queue/readme` 拉取插件 README Markdown，便于在界面内查阅行为说明。

### 5.4 Tab「队列观测」

V1 预留，后续接入队列长度、消费速率、回调失败等指标。

---

## 6. 租户配额配置（Admin API）

`TenantRateLimitTable` 组件已实现完整 CRUD，但尚未挂接到 Drawer「队列配额」Tab。可通过 HTTP API 或 curl 配置。

### 6.1 创建 / 更新配额

```bash
curl -X PUT http://127.0.0.1:18080/v1/admin/tenant-rate-limits \
  -H 'Content-Type: application/json' \
  -d '{
    "tenant": "demo",
    "model": "",
    "path": "",
    "policy": "",
    "rps": 10,
    "burst": 20,
    "cost": 1
  }'
```

### 6.2 字段说明

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `tenant` | 是 | — | 租户 ID，与 `X-Tenant-Id` 对应 |
| `model` | 否 | 空=通配 | 如 `gpt-4o` |
| `path` | 否 | 空=通配 | 如 `/v1/chat/completions` |
| `policy` | 否 | 空=通配 | `abandon` / `queue` / `wait` |
| `rps` | 是 | — | 每秒令牌恢复速率，>0 |
| `burst` | 是 | — | 突发容量（令牌桶大小），>0 |
| `cost` | 是 | 1 | 单次请求消耗令牌数，>0 |
| `ttl_secs` | 否 | 永久 | 临时配额过期秒数 |

**匹配优先级**：维度越具体越优先（带 `model+path+policy` 的规则优先于仅 `tenant` 的规则）。

Redis key 预览格式：

```text
ai:tenant:ratelimit:{tenant}[:model:...][:path:...][:policy:...]
```

### 6.3 查询与删除

```bash
# 列表（可按 tenant 过滤）
curl 'http://127.0.0.1:18080/v1/admin/tenant-rate-limits?tenant=demo'

# 删除（body 与创建时维度一致）
curl -X DELETE http://127.0.0.1:18080/v1/admin/tenant-rate-limits \
  -H 'Content-Type: application/json' \
  -d '{"tenant":"demo","rps":10,"burst":20,"cost":1}'
```

---

## 7. 客户端请求头

配置完成后，调用方经网关 `:9993` 发送请求时需携带：

| 请求头 | 必填 | 取值 | 说明 |
|--------|------|------|------|
| `X-RateLimit-Policy` | 当 `require=true` | `abandon` / `queue` / `wait` | 必须小写 |
| `X-Tenant-Id` | 建议 | 任意字符串 | 租户隔离与配额匹配 |
| `X-Callback-URL` | queue 超额时 | HTTPS URL | 异步回调地址 |
| `X-Model` | 否 | 模型名 | 影响优先级路由 |
| `X-Queue-Priority` | 否 | `high` / `normal` / `low` | 显式优先级 |

**三种策略行为（均需先过令牌桶）：**

| 策略 | 配额内 | 超额 |
|------|--------|------|
| `abandon` | 直通上游 200 | 429，不入队 |
| `queue` | 直通上游 200 | 202 + job_id，回调/轮询取结果 |
| `wait` | 直通上游 200 | 阻塞等待结果，超时 504 |

示例：

```bash
# abandon — 超额返回 429
curl -i http://127.0.0.1:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: abandon' \
  -H 'X-Tenant-Id: demo' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"hi"}'

# queue — 超额返回 202
curl -i http://127.0.0.1:9993/v1/chat/completions \
  -H 'X-RateLimit-Policy: queue' \
  -H 'X-Tenant-Id: demo' \
  -H 'X-Callback-URL: https://example.com/callback' \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"hi"}'
```

---

## 8. 配置与存储映射

```text
管理界面操作                    API                           存储位置
─────────────────────────────────────────────────────────────────────────
保存 AI 队列 Drawer      POST/PUT /config/plugin          plugin/wasm.ai-gateway-queue.json
保存网关                  PUT /config/item/{gw}/gateway     gateway/{gw}/config.json
保存路由规则              PUT .../route/item/{route}        gateway/{gw}/route/{route}.json
租户配额 PUT              PUT /v1/admin/tenant-rate-limits   Redis
读取 Schema               GET  /v1/admin/plugins/.../schema  （运行时生成）
```

文件命名规则：`{code}.{name}.json` → `wasm.ai-gateway-queue.json`。

---

## 9. 常见问题

### Q1：Schema 表单空白或加载失败？

检查 `VITE_AI_GATEWAY_BASE_URL` 是否指向运行中的 `ai-gateway-service`（默认 `http://127.0.0.1:18080`），并确认 `/healthz` 可访问。

### Q2：保存插件成功但请求未限流？

1. 是否在 **网关或路由** 中添加了插件引用？
2. 是否 **重复挂载** 导致行为异常？
3. SpaceGate 是否已加载最新配置（文件模式通常自动热更）？

### Q3：第一次请求就 429？

- 检查 Gateway + Route **双重挂载**
- 检查租户 `burst` 是否过小
- 用 Admin API 调高配额或新建租户规则

### Q4：缺 Policy 返回 400？

`plugin_config.policies.require=true`（默认）。客户端必须带 `X-RateLimit-Policy`，或在 Drawer 中关闭 require 并设置 default。

### Q5：保存插件配置报 `Read-only file system (os error 30)`？

**原因：** Docker 队列模式下 `admin-server` 配置卷被挂成 **只读（`:ro`）**，无法写入 `plugin/wasm.ai-gateway-queue.json`。

**修复：**

1. `docker-compose.queue.yml` 中 admin-server 使用 **整目录可写** 挂载（勿 `:ro`）：

```yaml
admin-server:
  volumes:
    - ./.docker/ai-gateway-demo:/etc/spacegate
```

2. Wasm 二进制挂到配置目录外，URL 用 `file:///opt/wasm/...`（见 `docker-compose.queue.yml` 注释）。

3. 重建 admin-server 镜像（含插件增量写入修复）后重启容器：

```bash
cd spacegate
docker build -f resource/docker/spacegate-admin/Dockerfile -t ai-gateway/admin-server:dev .
docker rm -f ai-gateway-admin-server && docker run -d --name ai-gateway-admin-server \
  --network container:ai-gateway-spacegate --restart unless-stopped \
  -e CONFIG=file:/etc/spacegate -e RUST_LOG=info \
  -v $(pwd)/../.docker/ai-gateway-demo:/etc/spacegate \
  ai-gateway/admin-server:dev -c file:/etc/spacegate -p 19992 -H 0.0.0.0
```

**临时绕过：** 直接编辑宿主机 `.docker/ai-gateway-demo/plugin/wasm.ai-gateway-queue.json`，无需走 UI 保存。

### Q6：`:9080` 管理端报 No such file or directory？

admin-server 读不到 `/etc/spacegate` 配置。Docker 环境检查 **工作区根目录** `.docker/ai-gateway-demo` 是否正确挂载到容器内 `/etc/spacegate`。

### Q7：K8s 环境能用这套 UI 吗？

可以管理 SpaceGate 配置（若 admin-server 连到同一配置源）。K8s 下 Wasm 常通过 **SgFilter** 内联 spec + HTTP/OCI 分发 Wasm，详见 [`deploy/README.md`](../../../deploy/README.md) §6。Higress **WasmPlugin** CR 的 `defaultConfig` **不含** `clusters`，生产建议用 **SgFilter**。

---

## 10. 推荐配置流程（ checklist ）

```text
□ Redis、ai-gateway-service、上游 LLM 已启动
□ 编译 Wasm 并确认 url 可访问
□ 设置 VITE_AI_GATEWAY_BASE_URL
□ 插件页 → AI → 配置 AI 请求队列网关 → 保存
□ 网关或路由（二选一）添加 wasm / ai-gateway-queue 引用
□ 路由后端指向 LLM 服务
□ （可选）PUT /v1/admin/tenant-rate-limits 配置租户配额
□ curl 冒烟：400（无 Policy）/ 200（配额内）/ 429（超额 abandon）
```

---

## 11. 相关源码索引

| 文件 | 说明 |
|------|------|
| `spacegate-admin-fe/components/config/src/components/PluginPanel.vue` | AI Tab 与 Drawer 入口 |
| `spacegate-admin-fe/components/config/src/components/AiGatewayQueueDrawer.vue` | 主配置 Drawer |
| `spacegate-admin-fe/components/config/src/components/TenantRateLimitTable.vue` | 租户配额表格（待接入 Tab） |
| `spacegate-admin-fe/components/config/src/api/aiGateway.ts` | ai-gateway-service Admin API 客户端 |
| `binary/ai-gateway-service/src/app/admin.rs` | Schema / Readme 端点 |
| `binary/ai-gateway-service/src/app/types.rs` | `AiGatewayQueuePluginConfig` 结构 |
| `resource/ai-gateway-demo/` | 文件模式配置模板 |
