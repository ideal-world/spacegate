# hai-scheduler 参考

所有节点必须先初始化同一共享 RelDB（跨主机生产使用 PostgreSQL/MySQL；SQLite 文件只适合可正确共享同一文件的本机进程）。数据库保存任务、执行 journal、全局任务占位和租约，`enableDb` 只能为 true；不再依赖缓存。`leaseMs` 默认 300000，范围 1000～86400000 毫秒，执行中自动续期。cron、手动和跨周期执行均争用同一任务槽，过期 owner 不能续活或提交。节点每个 `tickInterval` 刷新定义，`tasks` 只是最近成功快照；定义读取失败、删除、禁用、改期均中断旧尝试，已发生副作用不会回滚。

`context.occurrenceId` 是持久发生标识；同一 cron 分钟跨节点一致，手动可传 1～128 字符 `idempotencyKey` 重用原记录，省略时每次独立。`executionToken` 每次领取变化，`sequence` / `attempt` 提供执行代信息。API 会覆盖 `Idempotency-Key`、`X-Scheduler-Sequence`、`X-Scheduler-Attempt`、`X-Scheduler-Task`（任务 ID 使用 percent-encoding）。Hook 及生命周期回调已领取时的 `event.context` 使用同一发生标识；观测回调若产生副作用也须去重。

`idempotent` 默认为 false；只有接收端已将幂等账本和全部副作用原子持久提交，才声明 true。租约过期的已执行任务仅在 true 时以同 occurrenceId 接管；非幂等或耗尽本 occurrence 保存的 retry.maxAttempts（默认 1，范围 1–100，包含首次执行及恢复）时进入 `unknown` 并继续占住任务槽。租约无法强制撤销暂停进程已发出的外部操作，单有 HTTP 头不构成去重实现。重试同样受幂等声明约束。

`scheduler.execution.get(occurrenceId)` 查询权威状态；`listUnknown({ limit, offset })` 分页获取未知结果，limit 为 1～100，offset 为 0～1000000。可信人工先核实外部效果、确认旧 runner 已终止，再按 `expectedUpdatedAt` CAS 调用 `resolveUnknown(id, { status: 'success' | 'discarded', expectedUpdatedAt })` 释放占位；不能直接把 unknown 重排。`log.occurrenceId` 可作查询回执；`getLogs` 仅为尝试观测日志，日志保留策略不删除 journal/幂等历史。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端通过 API 端点触发 `scheduler.trigger()` 或查询任务日志。

## 依赖

| 模块 | 用途 | 是否必需 | 初始化要求 |
| --- | --- | --- | --- |
| `@h-ai/reldb` | 任务/执行 journal/租约 | 始终必需 | 先初始化同一共享数据库 |

## 使用步骤

### 1. 初始化

```ts
import { core } from '@h-ai/core'
import { reldb } from '@h-ai/reldb'
import { scheduler } from '@h-ai/scheduler'

await reldb.init({ type: 'sqlite', database: './scheduler.db' })

await scheduler.init({
  enableDb: true,
  maxLogs: 1000,
  retentionDays: 30,
  hooks: {
    onTaskStart(event) {
      core.logger.info('Task started', { taskId: event.task.id, trigger: event.trigger })
    },
    onTaskInterrupted(event) {
      core.logger.warn('Task interrupted', { taskId: event.task.id, reason: event.reason })
    },
    onTaskFinish(event) {
      core.logger.info('Task finished', { status: event.log.status })
    },
  },
})
```

### 2. 注册任务

```ts
await scheduler.register({
  id: 'health-check',
  name: '健康检查',
  description: '每 5 分钟巡检一次接口健康状态',
  cron: '*/5 * * * *',
  params: { channel: 'ops' },
  handler: {
    kind: 'api',
    url: 'https://api.example.com/health',
    method: 'GET',
  },
})

await scheduler.register({
  id: 'cleanup',
  name: '清理过期数据',
  description: '夜间单次清理任务',
  cron: '0 2 * * *',
  deleteAfterRun: true,
  params: { source: 'nightly' },
  handler: {
    kind: 'js',
    code: '(context) => ({ taskId: context.taskId, params: context.params })',
  },
})
```

> ⚠️ **安全警示**：`kind: 'js'` 仅允许受信任的服务端代码。当前实现在线程内使用 Node.js `vm`，超时可终止 Worker，但不构成安全沙箱；禁止把用户、租户或运营后台自由输入的 JS 字符串直接注册为任务。需要可配置执行逻辑时，优先改用 `kind: 'api'` 或 `hooks.onTaskExecute`。

### 3. 使用全局 execute hook 处理无 handler 任务

```ts
await scheduler.setHooks({
  async onTaskExecute(event) {
    return { via: 'hook', source: event.context.trigger.source }
  },
})

await scheduler.register({
  id: 'hook-task',
  name: 'Hook 任务',
  cron: '* * * * *',
})
```

### 4. 手工触发与日志查询

```ts
const result = await scheduler.trigger('cleanup', { source: 'admin-console' })

const logs = await scheduler.getLogs({
  taskId: 'cleanup',
  triggerType: 'manual',
  triggerSource: 'admin-console',
  startedAfter: Date.now() - 24 * 60 * 60 * 1000,
  startedBefore: Date.now(),
  pagination: { page: 1, pageSize: 20 },
})
```

## 核心 API

| 方法 / 属性 | 签名 | 说明 |
| --- | --- | --- |
| `init` | `(config?) => Promise<HaiResult<void>>` | 初始化调度器 |
| `register` | `(task) => Promise<HaiResult<void>>` | 注册统一任务模型 |
| `updateTask` | `(taskId, updates) => Promise<HaiResult<void>>` | 更新 cron / params / handler |
| `refreshTasks` | `() => Promise<HaiResult<void>>` | 主动刷新共享定义，不执行任务 |
| `register` 扩展字段 | `description / deleteAfterRun / retry / idempotent` | 支持任务描述、一次性任务、失败重试策略 |
| `trigger` | `(taskId, { source?, idempotencyKey? }) => Promise<HaiResult<TaskExecutionLog>>` | 手工触发并记录来源 |
| `execution.listLogFailures / repairLog` | `(options?) / (occurrenceId)` | 有界监控并幂等补写已发布 journal，不重执行业务 |
| `getLogs` | `(options?) => Promise<HaiResult<PaginatedResult<TaskExecutionLog>>>` | 支持按 trigger + startedAfter/startedBefore 过滤日志 |
| `setHooks` | `(hooks) => HaiResult<void>` | 设置全局生命周期回调 |
| `clearHooks` | `() => HaiResult<void>` | 清空全局生命周期回调 |
| `start / stop` | `() => HaiResult<void>` | 启动 / 停止调度 |
| `close` | `() => Promise<void>` | 关闭调度器 |

## 常见模式

### 持久化 JS 任务

```ts
await scheduler.init({ enableDb: true })

await scheduler.register({
  id: 'persisted-js',
  name: '持久化 JS 任务',
  cron: '*/10 * * * *',
  handler: {
    kind: 'js',
    code: '(context) => ({ taskId: context.taskId })',
  },
})
```

### 手工触发来源审计

```ts
await scheduler.trigger('cleanup', { source: 'admin-console' })
await scheduler.trigger('cleanup', { source: 'cli' })

const adminLogs = await scheduler.getLogs({ triggerSource: 'admin-console' })
```

### 共享执行租约

```ts
await reldb.init({ type: 'sqlite', database: './scheduler.db' })
await scheduler.init({
  enableDb: true,
  maxLogs: 500,
  retentionDays: 14,
  leaseMs: 300000,
  nodeId: 'node-1',
})
```

### 日志失败与补写

日志的 `id` 是每次尝试固定的 UUID，`persistence.status` 与业务 `status` 独立：`persisted` 表示日志及确认回执已持久化；`pending` 表示业务终态已提交但日志待补写；`unconfirmed` 表示没有已确认的业务终态，不能据此补写成功。只有 owner 与有效租约条件提交的终态会发布完整尝试列表，随后才投影日志；失租节点不能投影成功。日志失败不会把业务成功改成失败，也不会因此重跑业务。

补写会缩小 pending 列表；批量补写应反复读取 offset=0，避免一边补写一边增加 offset 跳过记录。

可信服务端用 `execution.listLogFailures({ limit, offset })` 监控 pending（范围与 listUnknown 相同），用 `execution.repairLog(occurrenceId)` 从共享 journal 幂等补写；可跨节点、重启及并发调用。写入已提交但回执丢失时通过固定主键和完整内容核对去重。补写返回 `LOG_PERSIST_FAILED`（032）时继续保留 pending，并输出安全告警；全部数据库不可用且终态未提交时仍按 unknown 核对外部效果，不能假定存在可恢复结果。补写与日志保留均不调用业务 Hook。保留策略失败仅告警，不撤销已保存日志。`onTaskFinish` 是本地业务完成通知，其 unconfirmed 状态不是持久化确认；以 trigger 返回值及 execution.get 的 logPersistence 为准。

领取前竞争/跳过的 interrupted 仅为观测日志，没有可补写的终态；其写入未确认时返回 unconfirmed。JSON 大载荷保存完整结果和全部重试记录，避免 MySQL 的 255 字符截断；新 schema 不自动迁移旧日志/执行表。关闭会等待在途补写，先关闭 scheduler 再关闭数据库。

```ts
const result = await scheduler.trigger('cleanup', { idempotencyKey: 'cleanup-2026-10-23' })
if (!result.success)
  return result
if (result.data.persistence.status === 'pending' && result.data.occurrenceId) {
  const repaired = await scheduler.execution.repairLog(result.data.occurrenceId)
  if (!repaired.success)
    return repaired
}
// 不用日志补写失败重新 trigger 业务；定期按页扫描 listLogFailures。
```

## 错误码 — `HaiSchedulerError`

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiSchedulerError.NOT_INITIALIZED` | `hai:scheduler:010` | 未初始化 |
| `HaiSchedulerError.INIT_FAILED` | `hai:scheduler:011` | 初始化失败 |
| `HaiSchedulerError.CONFIG_ERROR` | `hai:scheduler:012` | 配置错误 |
| `HaiSchedulerError.TASK_NOT_FOUND` | `hai:scheduler:020` | 任务不存在 |
| `HaiSchedulerError.TASK_ALREADY_EXISTS` | `hai:scheduler:021` | 任务已存在 |
| `HaiSchedulerError.INVALID_CRON` | `hai:scheduler:022` | Cron 表达式无效 |
| `HaiSchedulerError.EXECUTION_FAILED` | `hai:scheduler:023` | 执行失败 |
| `HaiSchedulerError.JS_EXECUTION_FAILED` | `hai:scheduler:024` | JS 执行失败 |
| `HaiSchedulerError.API_EXECUTION_FAILED` | `hai:scheduler:025` | API 执行失败 |
| `HaiSchedulerError.DB_SAVE_FAILED` | `hai:scheduler:026` | DB 保存失败 |
| `HaiSchedulerError.ALREADY_RUNNING` | `hai:scheduler:027` | 已在运行 |
| `HaiSchedulerError.NOT_RUNNING` | `hai:scheduler:028` | 未在运行 |
| `HaiSchedulerError.JS_COMPILE_FAILED` | `hai:scheduler:030` | JS 编译失败 |
| `HaiSchedulerError.HOOK_EXECUTION_FAILED` | `hai:scheduler:031` | Hook 执行失败 |
| `HaiSchedulerError.LOG_PERSIST_FAILED` | `hai:scheduler:032` | 日志持久化未确认 |

调度循环按分钟匹配 cron，同一分生成稳定的持久 occurrenceId。

数据库是共享租约的时间权威：claim/begin/renew/finish/expired/isolate 均在执行 SQL 内取存储端时钟并原子核验期限，不用节点 Date.now 或“先查 now 再写”的结果授权。PostgreSQL clock_timestamp、MySQL SYSDATE(3)、SQLite now 在真实锁等待后仍拒绝过期提交；updatedAt 条件版本使用同一数据库时钟并单调推进。cron 从 DB 时间生成分钟身份并匹配；默认 timezone 为 UTC，显式 IANA 时区只改变墙上时间解释，不读取节点 TZ。节点偏差 ±2h、不同 TZ 下同一分钟身份、过期但未接管、接管后的旧 owner 和行锁跨期限均须真实多进程验证。应用仍维护 DB 时钟与外部接收端的持久幂等/fence；未知效果不能靠补写日志或重新 trigger 重做。

公开使用以 @h-ai/scheduler 的实际入口与声明为准。声明 JSDoc 的公开示例检查 HaiResult，关闭调用使用 await scheduler.close()（返回 Promise<void>）；内部 repository/runner/compiler/cron 示例标注“模块内部上下文”，不能照抄为根导入。业务 status 与 persistence 分开，pending 补写不会重跑业务；unknown/unconfirmed 先核对共享 journal 与外部效果。应用使用所安装版本的公开类型，并通过本应用已有的类型检查验证使用示例。
