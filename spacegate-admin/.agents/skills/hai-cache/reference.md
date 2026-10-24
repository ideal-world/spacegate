# hai-cache 参考

> `@h-ai/cache` 提供统一缓存接口，支持 Memory 与 Redis 后端，包含 KV / Hash / List / Set / ZSet / 分布式锁 六类操作。

内存 KV 在到期时间点即失效。整数计数器拒绝 null/boolean/array、非整数增量和安全整数溢出，失败不修改原值；Hash 的 `__proto__` / `constructor` 名称按普通字段保留。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端无需直接操作缓存，由服务端模块（如 IAM、Scheduler）内部使用。

## 适用场景

- 缓存热点数据减少数据库查询
- 会话存储（配合 IAM）
- 分布式缓存（Redis）与本地测试缓存（Memory）
- 集合操作（权限缓存、标签集合）
- 排行榜（ZSet）与队列（List）
- 分布式锁（多节点部署互斥控制）

## 使用步骤

### 1. 配置

```yaml
# config/_cache.yml
type: memory # memory | redis
# Redis 配置（type=redis 时生效）：
# host: localhost
# port: 6379
# password: ''
# db: 0
# keyPrefix: 'hai:'
```

约定环境变量优先于 YAML，例如 `HAI_CACHE_TYPE`、`HAI_CACHE_HOST`、
`HAI_CACHE_KEYPREFIX`。

### 2. 初始化与关闭

```typescript
import { cache } from '@h-ai/cache'

await cache.init(core.config.get('cache'))
// 使用后关闭
const closed = await cache.close()
if (!closed.success) throw new Error(closed.error.message)
```


初始化只在健康检查成功后发布连接；失败时释放候选资源。若清理仍失败，初始化错误的 `error.ext.cleanupPending` 为 `true`。`cache.close()` 返回 `Promise<HaiResult<void>>`，等待正在进行的初始化，并合并并发关闭。开始清理后 `isInitialized` 为 `false`，配置与业务操作不再可用；清理失败仍保留资源，后续 `close()` 或 `init()` 先重试释放，成功前不会创建替代连接。调用方必须检查关闭结果。

```ts
const closed = await cache.close()
if (!closed.success) {
  // 处理清理故障后有限重试；再次失败应向应用报告。
  const retried = await cache.close()
  if (!retried.success) throw new Error(retried.error.message)
}
```


`cache.config` 返回的是脱敏后的配置快照；Redis `password` / `url` 等敏感值不会原样暴露给日志或调试输出。

## 核心 API

### KV 操作（`cache.kv`）

| 方法                                | 说明                 |
| ----------------------------------- | -------------------- |
| `get / set / del / exists`          | 基础读写与存在性判断 |
| `expire / expireAt / ttl / persist` | TTL 管理             |
| `incr / incrBy / decr / decrBy`     | 计数器操作           |
| `mget / mset`                       | 批量读写             |
| `scan / keys / type`                | Key 检索与类型判断   |

```typescript
await cache.kv.set('user:123', { name: '张三' }, { ex: 3600 })
const result = await cache.kv.get<{ name: string }>('user:123')
if (result.success && result.data) {
  // result.data.name === '张三'
}

await cache.kv.del('user:123')
```

### Cluster 多键与扫描

`mget/del/exists` 跨 slot 时有界并发且无整体原子性/快照；mget 保留输入顺序，DEL 失败可能已部分删除，重试幂等。`mset`、集合交/并/差和多键 blocking pop 须同 slot，否则返回 `CROSS_SLOT`；使用共同的 `{hash-tag}`，MSET 不做部分写入回退。

scan 首次传字符串 `'0'`，后续原样传回不透明游标，返回 `'0'` 表示完成。Cluster 覆盖所有 master；同配置和拓扑的另一应用节点可接续。拓扑、match 或 keyPrefix 改变时返回 `SCAN_TOPOLOGY_CHANGED`，从 `'0'` 重启；无效游标或 count 返回 `INVALID_SCAN`。返回去除 keyPrefix 的逻辑 key，前缀 glob 字符按字面量解释；keys 同样扫描并去重。SCAN 无快照，可能重复，count 仅为正整数提示值。

```ts
const first = await cache.kv.scan('0', { match: 'users:*', count: 100 })
if (first.success && first.data[0] !== '0') {
  const next = await cache.kv.scan(first.data[0], { match: 'users:*', count: 100 })
}
```

### Hash/List/Set/ZSet（`cache.hash/list/set_/zset`）

- `cache.hash`：对象字段读写（如用户 profile 局部更新）
- `cache.list`：队列/消息顺序处理
- `cache.set_`：去重集合（成员关系、权限集合）
- `cache.zset`：分数排序（排行榜、权重调度）

```typescript
await cache.hash.hset('profile:1', { nickname: 'alice' })
await cache.list.lpush('queue:jobs', 'job-1', 'job-2')
await cache.set_.sadd('role:admin:perms', 'user.read', 'user.write')
await cache.zset.zadd('rank:daily', { member: 'u1', score: 100 })
```

### 分布式锁（`cache.lock`）

| 方法       | 说明                                       |
| ---------- | ------------------------------------------ |
| `acquire`  | 尝试获锁（SET NX EX），返回唯一 owner 或 null |
| `release`  | 释放锁（必须携带本次 owner，防止误释放） |
| `isLocked` | 检查锁是否被持有                           |
| `extend`   | 续期锁 TTL（必须携带本次 owner） |

```typescript
// acquire 返回本次唯一 owner，后续释放/续期必须复用该凭据
const acquired = await cache.lock.acquire('my-lock', { ttl: 30 })
if (acquired.success && acquired.data) {
  const owner = acquired.data
  try {
    // 长任务仅在仍持有锁时续期；失败或 data=false 时停止依赖锁的后续操作
    const extended = await cache.lock.extend('my-lock', 60, owner)
    if (!extended.success || !extended.data) {
      logger.warn('Lock extension failed; protected work cancelled')
    }
    else {
      // 受保护的操作
    }
  }
  finally {
    await cache.lock.release('my-lock', owner)
  }
}

// 检查
const locked = await cache.lock.isLocked('my-lock')
```

**最佳实践：**

- `owner` 必须取自成功的 acquire.data；缓存内部每次生成唯一凭据，不能使用 nodeId 替代。释放/续期复用该凭据。
- 释放锁时传入 `owner` 防止误释放他人锁
- Memory 后端适合开发/测试；Redis 后端用 Lua 脚本保证 release/extend 原子性

## 错误码 — `HaiCacheError`

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiCacheError.CONNECTION_FAILED` | `hai:cache:001` | 连接失败 |
| `HaiCacheError.OPERATION_FAILED` | `hai:cache:002` | 操作失败 |
| `HaiCacheError.SERIALIZATION_FAILED` | `hai:cache:003` | 序列化失败 |
| `HaiCacheError.DESERIALIZATION_FAILED` | `hai:cache:004` | 反序列化失败 |
| `HaiCacheError.KEY_NOT_FOUND` | `hai:cache:005` | 键不存在 |
| `HaiCacheError.TIMEOUT` | `hai:cache:006` | 超时 |
| `HaiCacheError.NOT_INITIALIZED` | `hai:cache:010` | 未初始化 |
| `HaiCacheError.UNSUPPORTED_TYPE` | `hai:cache:011` | 不支持的缓存类型 |
| `HaiCacheError.CONFIG_ERROR` | `hai:cache:012` | 配置错误 |
| `HaiCacheError.INVALID_SCAN` | `hai:cache:013` | 游标或 count 无效 |
| `HaiCacheError.SCAN_TOPOLOGY_CHANGED` | `hai:cache:014` | 拓扑或条件变化，重新遍历 |
| `HaiCacheError.CROSS_SLOT` | `hai:cache:015` | 多键操作不在同一 slot |

## 常见模式

### 缓存命中与回源（未缓存空值，不防穿透）

```typescript
async function getUserCached(userId: string) {
  const cached = await cache.kv.get<User>(`user:${userId}`)
  if (cached.success && cached.data)
    return cached.data

  const user = await userRepo.findById(userId)
  if (user.success && user.data) {
    await cache.kv.set(`user:${userId}`, user.data, { ex: 3600 })
    return user.data
  }
  return null
}
```

### 分布式锁保护共享资源

```typescript
const lockKey = 'batch:import'
const acquired = await cache.lock.acquire(lockKey, { ttl: 60 })
if (acquired.success && acquired.data) {
  const owner = acquired.data
  try {
    await runBatchImportWithConditionalCommit()
  }
  finally {
    await cache.lock.release(lockKey, owner)
  }
}
else if (acquired.success) {
  logger.info('Another node is running the import')
}
else {
  logger.error('Lock acquisition failed', { code: acquired.error.code })
}
```

## 相关 Skills

- `hai-build`：模块初始化顺序（cache 在 iam 之前；cache 自身不依赖 reldb）
- `hai-core`：配置与 HaiResult 模型
- `hai-iam`：会话存储与权限缓存（底层使用 cache）
- `hai-scheduler`：定时任务分布式锁（底层使用 cache.lock）
- `hai-reach`：消息发送互斥锁（底层使用 cache.lock）
- `hai-kit`：SvelteKit 集成层

Cluster 与 Sentinel 数据节点认证使用 `password`，Cluster 发现的新节点继承同一鉴权配置。`url` 模式只使用 URL 内凭据，单独 password 不覆盖 URL。

单键 compareAndSwap(key, expected, value, {px?,keepTtl?}) 比较 JSON 序列化快照并原子替换，必须检查 success 与 boolean data。expected=undefined 表示不存在，value=undefined 表示删除，null 是值；px 正整数且与 keepTtl 互斥。比较失败不修改 TTL；跨节点版本状态优先使用 CAS 而不是租约后无条件 SET。

owner 只校验租约续期/释放，不是资源 fencing token。暂停或过期后旧任务仍可能执行，最终持久化写入必须使用资源自己的 CAS/代际条件/事务约束。批量导入等应用函数须在每次提交时校验条件，不能只靠锁包裹 Promise。

## 生产共享状态

生产仅使用 Redis（含 Cluster）；memory 只用于开发/明确测试。Redis 故障返回失败，不回退本机缓存。 NODE_ENV 或 HAI_CORE_ENV 为 production 时在初始化成功前检查，只有 HAI_E2E=1/HAI_E2E_MOCK=1 明确测试豁免。不要将租约当作最终写入条件，外部未知效果须按稳定业务键核对。

`kv.set` 返回 `HaiResult<boolean>`：`data=true` 表示实际写入，NX/XX 未命中返回 `success=true,data=false` 且不改变原值或 TTL。签发票据、会话或登记公钥前必须检查这两个字段。`ex/px/exat/pxat` 分别使用秒、毫秒、Unix 秒、Unix 毫秒，须为正安全整数且只能指定一种，与 `keepTtl=true` 互斥；`nx/xx` 也互斥。零、负数、小数、非有限值或溢出返回 `INVALID_SET_OPTIONS`（`cache:016`），不会写成永久键。过去的正绝对时间表示立即过期。省略期限会移除原 TTL；`keepTtl` 保留已有 TTL，新键仍永久保存。memory 和 Redis 都保存 JSON 快照。
