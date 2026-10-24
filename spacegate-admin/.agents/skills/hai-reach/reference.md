# hai-reach — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/reach` 提供统一的用户触达接口，支持同时注册多个 Provider（SMTP 邮件、短信、API 回调），内置模板引擎与免打扰（DND）机制。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端不直接发送邮件/短信，而是通过 API 端点触发服务端 `reach.send()`。

## 依赖

| 模块 | 用途 | 是否必需 | 初始化要求 |
| --- | --- | --- | --- |
| `@h-ai/reldb` | 数据库（发送日志与模板持久化） | DND delay 必需；其他场景可选 | 启用 delay 前先初始化，队列表不可用则 reach.init 返回 CONFIG_ERROR |
| `@h-ai/cache` | 缓存（DND delay 策略分布式锁） | 可选 | 已初始化时自动启用分布式锁 |

## 适用场景

- 同时使用邮件、短信、API 回调发送通知
- 发送邮件通知（注册欢迎、密码重置、告警等）
- 发送短信验证码或通知
- 通过 HTTP API 回调触发第三方通知
- 定义和管理消息模板（模板绑定到具体 Provider，支持配置文件定义）
- 免打扰时段控制
- 基于 `HaiReachError` 做错误分支处理

## 使用步骤

### 1. 配置

```yaml
# config/_reach.yml
providers:
  - name: email
    type: smtp
    host: ${HAI_REACH_PROVIDERS_0_HOST:smtp.example.com}
    port: ${HAI_REACH_PROVIDERS_0_PORT:465}
    secure: true
    user: ${HAI_REACH_PROVIDERS_0_USER:}
    pass: ${HAI_REACH_PROVIDERS_0_PASS:}
    from: ${HAI_REACH_PROVIDERS_0_FROM:noreply@example.com}
  - name: sms
    type: aliyun-sms
    accessKeyId: ${HAI_REACH_PROVIDERS_0_ACCESSKEYID:}
    accessKeySecret: ${HAI_REACH_PROVIDERS_0_ACCESSKEYSECRET:}
    signName: ${HAI_REACH_PROVIDERS_0_SIGNNAME:}
  - name: webhook
    type: api
    url: ${HAI_REACH_WEBHOOK_URL:}
    idempotent: false # 只有接收端按 Idempotency-Key 原子、持久化去重时才能设 true

# 模板（可选，也可通过代码注册）
templates:
  - name: verification_code
    provider: email
    subject: '验证码: {code}'
    body: '您的验证码是 {code}，有效期 {minutes} 分钟。'
  - name: sms_code
    provider: sms
    body: '验证码: {code}，{minutes} 分钟内有效。'

# 免打扰（可选）
dnd:
  enabled: true
  strategy: delay # 必须先初始化 reldb；确认持久化成功后才返回 deferred 与 queueId
  start: '22:00'
  end: '08:00'
```

`providers` 是数组，因此示例保留稳定、可读的显式厂商变量名；若同时设置
按索引生成的约定变量（如 `HAI_REACH_PROVIDERS_0_HOST`），约定变量优先。

API Provider 只接受 HTTP(S) URL。回调地址若来自不可信输入，业务层必须先做域名 allowlist，并由企业网络出口策略限制内网访问；不要在日志中记录完整 URL、收件人或第三方响应体。

### 2. 初始化与关闭

```typescript
import { cache } from '@h-ai/cache'
import { core } from '@h-ai/core'
import { reach } from '@h-ai/reach'
import { reldb } from '@h-ai/reldb'

// 当前配置启用了 delay：数据库和队列表为必需依赖。
const database = await reldb.init(core.config.get('db'))
if (!database.success)
  throw new Error(database.error.message)
const cached = await cache.init(core.config.get('cache')) // 可选，启用 flush 分布式锁
if (!cached.success)
  throw new Error(cached.error.message)

// 再初始化触达模块（自动检测已初始化的 reldb/cache 单例）
const initialized = await reach.init(core.config.get('reach'))
if (!initialized.success)
  throw new Error(initialized.error.message)
// ... 使用触达服务
await reach.close()
```

`reach.config` 返回的是脱敏后的配置快照；`api.url` / `endpoint`、授权头、SMTP `pass`、短信密钥等敏感值会自动隐藏。

### 3. 保存模板（模板绑定 Provider）

```typescript
// 通过代码保存（配置文件中的模板在 init 时自动注册）
await reach.template.save({
  name: 'verification_code',
  provider: 'email',
  subject: '验证码: {code}',
  body: '您的验证码是 {code}，有效期 {minutes} 分钟。',
})

await reach.template.saveBatch([
  { name: 'welcome', provider: 'email', subject: '欢迎 {userName}', body: '亲爱的 {userName}，欢迎使用 {appName}！' },
  { name: 'sms_code', provider: 'sms', body: '验证码: {code}，{minutes} 分钟内有效。' },
])
```

### 4. 发送消息

```typescript
// 使用模板发送邮件（指定 provider）
const result = await reach.send({
  provider: 'email',
  to: 'user@example.com',
  template: 'verification_code',
  vars: { code: '123456', minutes: '5' },
})
if (!result.success)
  throw new Error(result.error.message)
if (result.data.deferred) {
  // queueId 是 hai_reach_send_log.queue_id 中的回执 UUID；此时尚未投递。
  await savePendingReceipt(result.data.queueId)
}

// 直接发送邮件（无模板）
await reach.send({
  provider: 'email',
  to: 'user@example.com',
  subject: '通知',
  body: '<h1>Hello</h1>',
})

// 发送短信（通过 extra 传递 Provider 特有参数）
await reach.send({
  provider: 'sms',
  to: '13800138000',
  extra: { templateCode: 'SMS_123456' },
  vars: { code: '654321' },
})

// API 回调
await reach.send({
  provider: 'webhook',
  to: 'user@example.com',
  body: '{"event":"signup"}',
})
```

## 核心 API

### reach 对象

| 方法 / 属性           | 签名                                                     | 说明                               |
| --------------------- | -------------------------------------------------------- | ---------------------------------- |
| `reach.init`          | `(config: ReachConfigInput) => Promise<HaiResult<void>>`    | 初始化（注册多个 Provider）        |
| `reach.send`          | `(message: ReachMessage) => Promise<HaiResult<SendResult>>` | 发送消息（通过 provider 字段路由） |
| `reach.queue.get` | `(queueId: string) => Promise<HaiResult<ReachQueueEntry \| null>>` | 不含正文/收件人的队列状态 |
| `reach.queue.list` | `(options: ReachQueueListOptions) => Promise<HaiResult<ReachQueueEntry[]>>` | 可信服务端分页查询 dead/unknown，每页至多 100 条 |
| `reach.queue.requeueDead` | `(queueId: string, options: ReachDeadRequeue) => Promise<HaiResult<void>>` | 按 expectedUpdatedAt 条件恢复 dead；unknown 禁止重排 |
| `reach.queue.resolveUnknown` | `(queueId: string, resolution: ReachUnknownResolution) => Promise<HaiResult<void>>` | 人工核对后将 unknown 裁定为 sent/discarded，不发送 |
| `reach.template`      | `ReachTemplateRegistry`                                  | 模板注册表                         |
| `reach.config`        | `ReachConfig \| null`                                    | 当前脱敏配置快照                   |
| `reach.isInitialized` | `boolean`                                                | 是否已初始化                       |
| `reach.close`         | `() => Promise<void>`                                    | 关闭所有连接                       |

### ReachConfigInput

```typescript
interface ReachConfigInput {
  providers: ProviderConfig[] // 多个 Provider 配置
  templates?: TemplateConfig[] // 通过配置文件定义的模板
  dnd?: {
    enabled: boolean // 是否启用
    strategy: 'discard' | 'delay' // discard 丢弃 / delay 延时发送
    start: string // 开始时间 HH:mm
    end: string // 结束时间 HH:mm
  }
}
```

### ReachMessage

| 字段       | 类型                      | 必填 | 说明                                |
| ---------- | ------------------------- | ---- | ----------------------------------- |
| `provider` | `string`                  | 条件 | 模板发送省略时由模板推导；直接发送必填；显式值优先 |
| `to`       | `string`                  | ✅   | 接收方（邮箱或手机号）              |
| `subject`  | `string`                  | —    | 邮件主题（直接发送时）              |
| `body`     | `string`                  | —    | 消息正文（直接发送时）              |
| `template` | `string`                  | —    | 模板名称（模板发送时）              |
| `vars`     | `Record<string, string>`  | —    | 模板变量                            |
| `extra`    | `Record<string, unknown>` | —    | Provider 扩展参数（如短信模板编码） |
| `idempotencyKey` | `string` | — | 直接 API 请求可指定；队列投递由框架固定为 queueId |

### ReachTemplate（模板绑定 Provider）

| 字段       | 类型     | 必填 | 说明                 |
| ---------- | -------- | ---- | -------------------- |
| `name`     | `string` | ✅   | 模板名称             |
| `provider` | `string` | ✅   | 绑定的 Provider 名称 |
| `subject`  | `string` | —    | 邮件主题模板         |
| `body`     | `string` | ✅   | 正文模板             |

## 错误码

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiReachError.SEND_FAILED`            | `hai:reach:001`   | 发送失败        |
| `HaiReachError.DELIVERY_REJECTED`      | `hai:reach:020`   | API 明确永久拒绝 |
| `HaiReachError.TEMPLATE_NOT_FOUND`     | `hai:reach:002`   | 模板未找到      |
| `HaiReachError.TEMPLATE_RENDER_FAILED` | `hai:reach:003`   | 模板渲染失败    |
| `HaiReachError.INVALID_RECIPIENT`      | `hai:reach:004`   | 无效接收方      |
| `HaiReachError.PROVIDER_NOT_FOUND`     | `hai:reach:005`   | Provider 未找到 |
| `HaiReachError.DND_BLOCKED`            | `hai:reach:006`   | 免打扰丢弃      |
| `HaiReachError.NOT_INITIALIZED`        | `hai:reach:010`   | 模块未初始化    |
| `HaiReachError.UNSUPPORTED_TYPE`       | `hai:reach:011`   | 不支持的类型    |
| `HaiReachError.CONFIG_ERROR`           | `hai:reach:012`   | 配置错误        |

## 常见模式

### 可靠延迟入队

启用 delay 后，只有数据库确认插入一条 pending 记录才返回 `deferred: true` 和 `queueId`。缺库、断连、插入失败返回失败 HaiResult，不能向用户报告已入队。`queueId` 对应 `hai_reach_send_log.queue_id`，同一行的 `status` 为 pending / processing / sending / unknown / sent / discarded / dead。节点重启后连接同一数据库、配置同名 Provider，可在 DND 时段外恢复投递。即时发送后的普通日志仍为可选，日志失败不会使已发送消息返回失败。

验证码和密码重置等有时效的通知应明确选择 DND 策略；可靠入队成功不代表用户已收到验证码。

队列 `created_at` / `updated_at` / `locked_until` / `next_attempt_at` 均为 Unix 毫秒 `BIGINT`（SQLite 为 64 位 INTEGER）；仓库把 driver 的字符串/数字归一化为安全整数 number，并拒绝负数、小数和不安全整数。模板仓库的 TIMESTAMP 时间返回 Date。已有表须在部署时同步列结构，包含次数、退避策略和失败分类，模块不自动迁移旧表；跨库验收必须运行 PostgreSQL/MySQL 用例，SQLite 无法证明 32 位溢出已经消除。

### 租约、接收端幂等与未知结果

每轮最多 100 条，逐条 claim，发送前原子写入 sending 并在途续租；过期 owner 的开始、续租、确认和释放均被拒绝。DND 时段外每分钟恢复遗留任务。cache 锁只是扫描优化，不能提供外部副作用的 exactly-once。

API `idempotent: true` 是接收端能力声明：接收方必须把 queueId 对应的去重记录与业务副作用原子、持久化提交。所有尝试用相同 `Idempotency-Key` 头和 `idempotencyKey` 正文，覆盖响应丢失和旧节点恢复；去重记录须覆盖所有迟到请求，迟到无上界时不能过期。同名 Provider 的目标与幂等存储保持一致。只有此模式能自动接管已开始的 API 投递。

SMTP、短信和默认 API 一旦开始投递，失败或过期转 unknown，不自动重发。未连接按持久策略退避；发送前解析失败直接进入 dead。unknown 必须在可信服务端管理流程里，先停止/隔离旧 worker 并核对第三方投递记录，再执行以下二选一裁定；discarded 不撤销已开始请求。需要补发时，先确保旧请求不可能继续生效，再创建新消息。

```ts
const state = await reach.queue.get(queueId)
const resolved = await reach.queue.resolveUnknown(queueId, { status: 'sent', messageId: verifiedMessageId })
if (!resolved.success)
  throw new Error(resolved.error.message)
```

管理入口校验回执 UUID、status 与 messageId 长度，只能从 unknown 条件更新为 sent/discarded；不提供重排队选项，并发裁定仅一个成功。

### 有限重试与死信运维

`queue.maxAttempts` 默认 5（1–100），`retryDelayMs` 默认 60000，`maxRetryDelayMs` 默认 3600000；两项毫秒配置范围 1000–86400000，最大退避不得小于首次退避。策略随入队记录保存，不随新节点配置重置；领取（含崩溃恢复）原子累加 attemptCount。暂时故障保存 nextAttemptAt，按 `min(maxRetryDelayMs, retryDelayMs × 2^(attemptCount−1))` 退避。DND 外每分钟扫描，一轮最多 100 条且固定资格时间，失败行不会在同一轮反复领取。缺失/未连接 Provider、网络错误、HTTP 408/425/429/5xx 允许有限重试；坏 JSON、幂等 API 其它 4xx 直接 dead；超限（含最后一次领取后崩溃）停止自动处理。非幂等在途仍隔离 unknown，不能借重试预算重发。

queue.get/list 返回 attemptCount、maxAttempts、nextAttemptAt、lastFailure、updatedAt 等安全元数据。lastFailure 为 provider_unavailable、invalid_message、permanent_rejection、transient_failure 或 attempts_exhausted，不保存响应正文。可信服务端以 `list({ status: 'dead', limit: 50, offset: 0 })` 分页查询，status 也可选 unknown；limit 1–100，offset 0–1000000。

```ts
// 应用先校验运维权限并修复根因；不向浏览器直接暴露队列管理。
const state = await reach.queue.get(queueId)
if (!state.success)
  throw new Error(state.error.message)
if (state.data?.status === 'dead' && state.data.updatedAt !== undefined) {
  const restored = await reach.queue.requeueDead(queueId, { expectedUpdatedAt: state.data.updatedAt })
  if (!restored.success)
    throw new Error(restored.error.message)
}
```

恢复仅匹配当前 dead 版本，过时/并发请求失败；重置次数但保留 queueId 和原策略。unknown 永远不能通过此入口重排。dead 不代表外部副作用未发生，旧幂等 API 请求仍可能迟到，恢复前确认接收端持久去重；不可修复的坏消息留在 dead 人工排查。

### 多渠道验证码发送

```typescript
reach.template.save({
  name: 'email_code',
  provider: 'email',
  subject: '验证码: {code}',
  body: '您的验证码是 {code}，有效期 {minutes} 分钟。',
})

reach.template.save({
  name: 'sms_code',
  provider: 'sms',
  body: '验证码: {code}，{minutes} 分钟内有效。',
})

// 根据用户选择的渠道发送
async function sendCode(channel: 'email' | 'sms', target: string, code: string) {
  const template = channel === 'email' ? 'email_code' : 'sms_code'
  return reach.send({
    to: target,
    template,
    vars: { code, minutes: '5' },
  })
}
```

### 与 IAM 集成（密码重置 / OTP 验证码）

```typescript
import { iam } from '@h-ai/iam'
import { reach } from '@h-ai/reach'

// 初始化 reach（注册 email 和 sms Provider）
const initializedReach = await reach.init({
  providers: [
    { name: 'email', type: 'smtp', host: 'smtp.example.com', from: 'noreply@example.com' },
    { name: 'sms', type: 'aliyun-sms', accessKeyId: '...', accessKeySecret: '...', signName: '...' },
  ],
  templates: [
    { name: 'password_reset', provider: 'email', subject: 'Password Reset', body: 'Token: {token}, expires: {expiresAt}' },
    { name: 'otp_email', provider: 'email', subject: 'Code: {code}', body: 'Your code is {code}' },
    { name: 'otp_sms', provider: 'sms', body: 'Your code is {code}' },
  ],
})
if (!initializedReach.success)
  throw new Error(initializedReach.error.message)

// 先确认 reldb/cache/Reach 已初始化成功，再注入 Reach；内置适配器传播发送失败。
const initializedIam = await iam.init({ ...iamSettings, reach })
if (!initializedIam.success)
  throw new Error(initializedIam.error.message)
```

若使用显式通知回调覆盖内置适配器，必须检查 `reach.send()` 的 HaiResult，失败时拒绝 Promise；不得吞掉入队失败。需要即时送达的验证码不应启用延迟策略。

### 错误处理

```typescript
import { HaiReachError } from '@h-ai/reach'

const result = await reach.send(message)
if (!result.success) {
  switch (result.error.code) {
    case HaiReachError.NOT_INITIALIZED.code:
      break
    case HaiReachError.PROVIDER_NOT_FOUND.code:
      break
    case HaiReachError.DND_BLOCKED.code:
      // 免打扰时段（discard 策略），消息已丢弃
      break
    case HaiReachError.TEMPLATE_NOT_FOUND.code:
      break
    case HaiReachError.SEND_FAILED.code:
      break
  }
}
```

## 相关 Skills

- `hai-core` — 配置加载、日志、HaiResult 模式
- `hai-reldb` — 数据库操作（存储发送记录等）
- `hai-cache` — 缓存操作、分布式锁（DND flush 互斥）

## 生产共享状态

生产拒绝 console Provider，须配置真实 SMTP/短信/API 服务。console 的日志成功不是外部投递成功；非幂等服务的未知结果必须核对，不能自动重发证明 exactly-once。 NODE_ENV 或 HAI_CORE_ENV 为 production 时在初始化成功前检查，只有 HAI_E2E=1/HAI_E2E_MOCK=1 明确测试豁免。不要将租约当作最终写入条件，外部未知效果须按稳定业务键核对。
