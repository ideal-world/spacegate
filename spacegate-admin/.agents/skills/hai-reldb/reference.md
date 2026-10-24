# hai-reldb — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/reldb` 提供统一的数据库操作接口，支持 SQLite、PostgreSQL、MySQL，包含 DDL、SQL、CRUD 抽象、事务与分页。


派生 `BaseReldbCrudRepository` 的自定义 SQL 方法先检查 `await this.ensureReady()`；构造函数启动自动建表不等于仓库就绪，服务 init 必须等待候选仓库的就绪入口再发布。PostgreSQL 并发首次 `CREATE TABLE IF NOT EXISTS` 的 `23505/pg_type_typname_nsp_index` 目录冲突仅重执行相同 DDL 一次，其他错误继续失败。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端请通过 `@h-ai/api-contract` + `@h-ai/serv` 暴露的 API 端点间接访问数据库。

## 适用场景

- 新增或修改数据库访问逻辑（SQL/DDL/CRUD/事务）
- 使用 `reldb.crud.table` 或 `BaseReldbCrudRepository` 构建数据仓库
- 处理分页查询与分页结果规范化
- 基于 `HaiReldbError` 做错误分支处理
- JSON 列的路径提取、设置、插入、删除、合并操作（跨数据库统一语法）

## 使用步骤

### 1. 配置

只做校验时从 `@h-ai/reldb/schemas` 导入同一组 Schema/类型；该入口只依赖 Zod，无数据库实例或驱动。MySQL `ssl` 只接受 `false` 或严格对象；`{}` 默认开启证书链和 DNS 身份验证，不能用 `true` 或 PostgreSQL 字符串模式。实际连接仍需服务端驱动。

```yaml
# config/_db.yml
type: sqlite
database: ./data/app.db
# PostgreSQL/MySQL 额外字段：
# host: localhost
# port: 5432
# user: postgres
# password: ''
operationLog:
  read: false # query / get / queryPage
  write: false # DDL, execute / batch, tx.begin / tx.commit / tx.rollback
  maxLength: 1000
  level: debug # info | debug | trace
```

约定环境变量优先于 YAML，例如 `HAI_DB_TYPE`、`HAI_DB_DATABASE`、
`HAI_DB_OPERATIONLOG_LEVEL`。

`operationLog` 在 reldb provider-base 的 DML 操作层统一输出日志，不要在 `reldb-main.ts`、各 raw provider 或调用方重复包装。`read` 覆盖 `query/get/queryPage`，`write` 覆盖 DDL、`execute/batch` 与 `tx.begin/commit/rollback`，`maxLength` 限制安全数量摘要的字符数（0省略摘要）；SQL原文、参数值、DDL默认值和驱动错误内容不写入日志，`level` 默认 `debug`。

### 2. 初始化与关闭

MySQL `pool` 仅支持 `{ max: 10, acquireTimeout: 10000, queueLimit: 1000 }`（也是省略时默认值）。`acquireTimeout` 为 1–2147483647 毫秒的整数，覆盖排队和建连；queueLimit 为正整数。SQL/DDL/CRUD/batch/tx 共用有界 FIFO，超时移除并返回 `TIMEOUT`，满额返回 `POOL_EXHAUSTED`；迟到建连结算前仍占槽位，只归还连接，不执行 SQL。close 立即取消排队，再等待借用/事务和驱动有界建连完成，不中止已启动 SQL。min/idleTimeout/mysql.timezone 明确拒绝，不承诺 MySQL 最小保留或可配置空闲回收；PostgreSQL 保留自己的 min/idleTimeout/acquireTimeout=0 契约，min 不会预建连接。

MySQL 仅传 mysql:// URL 即可连接，优先于 host/port/database/user/password；无 URL 时 database 必填。禁止 URL query/fragment，pool/mysql.charset/ssl/operationLog 仍独立适用。ssl 仅 false 或严格对象，空对象默认校验证书链和 DNS 身份；ca/cert/key 为 PEM 内容，cert/key 必须成对。当前驱动不验证 IP 身份，验证开启的 IP 目标返回 CONFIG_ERROR；关闭验证有固定风险日志，加密不代表身份认证。环境变量为 HAI_DB_POOL_* / HAI_DB_SSL_*，min/idleTimeout 仅适用 PostgreSQL，queueLimit/verifyIdentity 仅适用 MySQL。

```ts
// trustedCaPem 从部署信任配置提供。
const result = await reldb.init({ type: 'mysql', host: 'db.example.com', database: 'app', pool: { max: 10, acquireTimeout: 5000, queueLimit: 100 }, ssl: { ca: trustedCaPem } })
if (!result.success) return result
```

PostgreSQL 空闲连接错误已监听，由池移除并在下一次查询替换；日志仅含固定说明与安全 code/name，不记录驱动错误原文。关闭会等待已借出事务完成，应用必须提交/回滚并释放句柄。

```typescript
import { core } from '@h-ai/core'
import { reldb } from '@h-ai/reldb'

await reldb.init(core.config.get('db'))
// ... 使用数据库
const closed = await reldb.close()
if (!closed.success) throw new Error(closed.error.message)
```


初始化只在健康检查成功后发布连接；失败时释放候选资源。若清理仍失败，初始化错误的 `error.ext.cleanupPending` 为 `true`。`reldb.close()` 返回 `Promise<HaiResult<void>>`，等待正在进行的初始化，并合并并发关闭。开始清理后 `isInitialized` 为 `false`，配置与业务操作不再可用；清理失败仍保留资源，后续 `close()` 或 `init()` 先重试释放，成功前不会创建替代连接。调用方必须检查关闭结果。

```ts
const closed = await reldb.close()
if (!closed.success) {
  // 处理清理故障后有限重试；再次失败应向应用报告。
  const retried = await reldb.close()
  if (!retried.success) throw new Error(retried.error.message)
}
```


`reldb.config` 返回脱敏后的配置快照；数据库密码、URL 内嵌凭证和 TLS 私钥 `ssl.key` 会隐藏，不能用快照回放原始连接配置。

### 3. 选择操作接口

| 接口 | 用途               | 入口                         |
| ---- | ------------------ | ---------------------------- |
| DDL  | 建表/索引/字段变更 | `reldb.ddl`                     |
| SQL  | 原始查询/执行/分页 | `reldb.sql`                     |
| CRUD | 通用增删改查       | `reldb.crud.table(config)`      |
| 仓库 | 业务数据仓库封装   | `extends BaseReldbCrudRepository` |
| 事务 | 事务管理           | `reldb.tx`                      |
| 分页 | 分页参数与结果     | `reldb.pagination`              |
| JSON | JSON 路径操作      | `reldb.json`                    |

## 核心 API

事务边界：`tx.wrap` 对正常返回值一律提交，不识别失败 HaiResult；回调内检查每步结果，失败抛错以触发回滚（外层返回 TRANSACTION_FAILED）。需保留原错误码时使用手动事务。

### DDL — `reldb.ddl`

| 方法          | 签名                                                                    | 说明                       |
| ------------- | ----------------------------------------------------------------------- | -------------------------- |
| `createTable` | `(tableName, columns: ReldbTableDef, ifNotExists?: boolean) => HaiResult<void>` | 建表（默认 IF NOT EXISTS） |
| `dropTable`   | `(tableName, ifExists?: boolean) => HaiResult<void>`                       | 删除表（默认 IF EXISTS）   |
| `addColumn`   | `(tableName, columnName, columnDef: ReldbColumnDef) => HaiResult<void>`         | 添加列                     |
| `dropColumn`  | `(tableName, columnName) => HaiResult<void>`                               | 删除列                     |
| `renameTable` | `(oldName, newName) => HaiResult<void>`                                    | 重命名表                   |
| `createIndex` | `(tableName, indexName, indexDef: ReldbIndexDef) => HaiResult<void>`            | 创建索引                   |
| `dropIndex`   | `(indexName, ifExists?: boolean) => HaiResult<void>`                       | 删除索引                   |
| `raw`         | `(sql: string) => HaiResult<void>`                                         | 执行原始 DDL               |

> 所有方法均返回 `Promise<HaiResult<void>>`，上表省略异步与错误类型。

**ReldbTableDef**（列名到列定义的映射）：

```typescript
const columns: ReldbTableDef = {
  id: { type: 'INTEGER', primaryKey: true, autoIncrement: true },
  name: { type: 'TEXT', notNull: true },
  email: { type: 'TEXT', unique: true },
  score: { type: 'REAL', defaultValue: 0 },
  active: { type: 'BOOLEAN' },
  metadata: { type: 'JSON' },
  created_at: { type: 'TIMESTAMP' },
}
```

**ReldbColumnDef**（列类型与约束）：

```typescript
interface ReldbColumnDef {
  type: 'TEXT' | 'INTEGER' | 'BIGINT' | 'REAL' | 'BLOB' | 'BOOLEAN' | 'TIMESTAMP' | 'JSON'
  primaryKey?: boolean
  autoIncrement?: boolean
  notNull?: boolean
  unique?: boolean
  defaultValue?: string | number | boolean | null
  references?: { table: string, column: string, onDelete?: 'CASCADE' | 'SET NULL' | 'RESTRICT' | 'NO ACTION', onUpdate?: '...' }
}
```

**跨数据库类型映射**：

| 类型      | SQLite  | PostgreSQL       | MySQL        |
| --------- | ------- | ---------------- | ------------ |
| TEXT      | TEXT    | TEXT             | VARCHAR(255) |
| INTEGER   | INTEGER | INTEGER/BIGSERIAL | INT/BIGINT   |
| BIGINT    | INTEGER（64 位） | BIGINT | BIGINT |
| REAL      | REAL    | DOUBLE PRECISION | DOUBLE       |
| BLOB      | BLOB    | BYTEA            | BLOB         |
| BOOLEAN   | INTEGER | BOOLEAN          | TINYINT(1)   |
| TIMESTAMP | INTEGER | TIMESTAMP        | DATETIME     |
| JSON      | TEXT    | JSONB            | JSON         |

> MySQL 将 TEXT 映射为 VARCHAR(255) 以支持索引和 UNIQUE 约束。INTEGER + autoIncrement 在 MySQL 映射为 BIGINT。

Unix 毫秒使用 BIGINT，不能使用 PostgreSQL/MySQL 的普通 INTEGER。原始 SQL/CRUD 与 BaseReldbCrudRepository 的 BIGINT 保留 driver 的 number/string/bigint 类型；业务需要 number 时须在读取边界校验非负整数和 Number.isSafeInteger，拒绝精度丢失。TIMESTAMP 经 BaseReldbCrudRepository 映射为 Date。

### SQL — `reldb.sql`

| 方法        | 签名                                                              | 说明         |
| ----------- | ----------------------------------------------------------------- | ------------ |
| `query`     | `<T>(sql, params?) => HaiResult<T[]>`                                | 查询返回多行 |
| `get`       | `<T>(sql, params?) => HaiResult<T \| null>`                          | 查询返回单行 |
| `execute`   | `(sql, params?) => HaiResult<ExecuteResult>`                         | 执行写操作   |
| `batch`     | `(statements: Array<{ sql, params? }>) => HaiResult<void>`           | 批量执行     |
| `queryPage` | `<T>(options: PaginationQueryOptions) => HaiResult<PaginatedResult>` | 分页查询     |

> 所有方法均返回 `Promise<HaiResult<T>>`，上表省略异步与错误类型。

**参数占位符统一使用 `?`**（PostgreSQL 的 `$n` 由 Provider 自动转换）：

```typescript
const result = await reldb.sql.query<User>(
  'SELECT * FROM users WHERE status = ? AND age > ?',
  ['active', 18],
)
```

**ExecuteResult**：

```typescript
interface ExecuteResult {
  changes: number // 影响的行数
  lastInsertRowid?: number | bigint // INSERT 时生效（PostgreSQL 不返回此字段）
}
```

**分页查询**：

```typescript
const page = await reldb.sql.queryPage<User>({
  sql: 'SELECT * FROM users WHERE status = ? ORDER BY id',
  params: ['active'],
  pagination: { page: 1, pageSize: 20 },
  overrides: { maxPageSize: 100 }, // 可选：限制最大页大小
})
// page.data => { items, total, page, pageSize }
```

### CRUD — `reldb.crud.table(config)`

通过配置创建单表 CRUD 仓库，免写 SQL：

`CrudConfig.dbType` 必填，必须与实际初始化的数据库后端一致（含环境变量覆盖后的类型）。以下为 SQLite 示例；切换到 PostgreSQL/MySQL 时同步修改。`BaseReldbCrudRepository` 会从已初始化的 `reldb.config.type` 获取类型，无需手动传入。

```typescript
const userCrud = reldb.crud.table<{ id: number, name: string, email: string }>({
  table: 'users',
  idColumn: 'id',           // 主键列，默认 'id'
  select: ['id', 'name', 'email'],   // 查询列，默认 '*'
  createColumns: ['name', 'email'],   // 允许插入的列
  updateColumns: ['name', 'email'],   // 允许更新的列
  dbType: 'sqlite',   // 必填，与本例初始化后端一致
})
```

返回的 ReldbCrudRepository 方法：

| 方法         | 签名                                           | 说明         |
| ------------ | ---------------------------------------------- | ------------ |
| `create`     | `(data, tx?) => HaiResult<ExecuteResult>`         | 创建单条     |
| `createMany` | `(items, tx?) => HaiResult<void>`                 | 批量创建     |
| `findById`   | `(id, tx?) => HaiResult<T \| null>`               | 按 ID 查询   |
| `findAll`    | `(options?, tx?) => HaiResult<T[]>`               | 条件查询     |
| `findPage`   | `(options, tx?) => HaiResult<PaginatedResult<T>>` | 分页查询     |
| `updateById` | `(id, data, tx?) => HaiResult<ExecuteResult>`     | 按 ID 更新   |
| `deleteById` | `(id, tx?) => HaiResult<ExecuteResult>`           | 按 ID 删除   |
| `count`      | `(options?, tx?) => HaiResult<number>`            | 计数         |
| `exists`     | `(options?, tx?) => HaiResult<boolean>`           | 条件是否存在 |
| `existsById` | `(id, tx?) => HaiResult<boolean>`                 | ID 是否存在  |

> 所有方法均支持可选 `tx` 事务参数。`create`/`updateById` 中的 `data` 会根据 `createColumns`/`updateColumns` 白名单过滤列。

**使用示例**：

```typescript
// 单条插入
const result = await userCrud.create({ name: '张三', email: 'test@example.com' })
// result.data → { changes: 1, lastInsertRowid: 1 }

// 批量插入
await userCrud.createMany([
  { name: '用户A', email: 'a@test.com' },
  { name: '用户B', email: 'b@test.com' },
])

// 主键查找
const user = await userCrud.findById(1)
// user.data → { id: 1, name: '张三', email: 'test@example.com' } | null

// 条件查询（where + params 占位符）
const actives = await userCrud.findAll({
  where: 'name LIKE ?',
  params: ['%张%'],
  orderBy: 'id DESC',
  limit: 10,
  offset: 0,
})

// 分页查询
const page = await userCrud.findPage({
  where: 'name LIKE ?',
  params: ['%张%'],
  orderBy: 'id DESC',
  pagination: { page: 1, pageSize: 20 },
})
// page.data → { items: [...], total: 100, page: 1, pageSize: 20 }

// 更新 / 删除
await userCrud.updateById(1, { name: '新名字' })
await userCrud.deleteById(1)

// 计数 / 存在性
const total = await userCrud.count({ where: 'name LIKE ?', params: ['%张%'] })
const has = await userCrud.exists({ where: 'email = ?', params: ['test@example.com'] })
const found = await userCrud.existsById(1)

// 事务中使用（所有方法均支持 tx 参数）
await reldb.tx.wrap(async (tx) => {
  const created = await userCrud.create({ name: '用户A', email: 'a@test.com' }, tx)
  if (!created.success) throw new Error(created.error.message)
  const updated = await userCrud.updateById(1, { name: '新名字' }, tx)
  if (!updated.success) throw new Error(updated.error.message)
  const user = await userCrud.findById(1, tx)
  if (!user.success) throw new Error(user.error.message)
  return user.data
})
```

### BaseReldbCrudRepository

业务仓库基类，提供字段映射、自动建表与类型转换能力：

```typescript
import { BaseReldbCrudRepository } from '@h-ai/reldb'

interface User { id: number, name: string, email: string, createdAt: Date, updatedAt: Date }

class UserRepository extends BaseReldbCrudRepository<User> {
  constructor() {
    super(db, {
      table: 'users',
      idColumn: 'id',
      fields: [
        { fieldName: 'id', columnName: 'id', def: { type: 'INTEGER', primaryKey: true, autoIncrement: true }, select: true, create: false, update: false },
        { fieldName: 'name', columnName: 'name', def: { type: 'TEXT', notNull: true }, select: true, create: true, update: true },
        { fieldName: 'email', columnName: 'email', def: { type: 'TEXT', notNull: true }, select: true, create: true, update: true },
        { fieldName: 'createdAt', columnName: 'created_at', def: { type: 'TIMESTAMP', notNull: true }, select: true, create: true, update: false },
        { fieldName: 'updatedAt', columnName: 'updated_at', def: { type: 'TIMESTAMP', notNull: true }, select: true, create: true, update: false },
      ],
    })
  }

  /** 自定义查询示例 */
  async findByEmail(email: string, tx?: DmlWithTxOperations) {
    return this.sql(tx).get<User>('SELECT * FROM users WHERE email = ?', [email])
  }
}
```

**`this.sql(tx?)`**：返回 `DmlOperations`（`reldb.sql` 或传入的事务句柄），自动适配事务场景。

**自动能力**：

- `createdAt` / `updatedAt` 字段自动填充时间戳
- 主键生成（非 autoIncrement 主键默认使用随机 UUID，底层带安全上下文守卫，不可用时回退；业务侧手动生成 UUID 一律用 `core.id.uuid()`）
- BOOLEAN → 1/0（SQLite/MySQL）或 true/false（PostgreSQL）
- TIMESTAMP → 毫秒时间戳（SQLite）或 Date（PG/MySQL）
- JSON → 字符串序列化（SQLite/MySQL）或原生 JSONB（PG）

### 事务 — `reldb.tx`

```typescript
// 方式1：wrap（自动 commit/rollback）
const result = await reldb.tx.wrap(async (tx) => {
  const inserted = await tx.execute('INSERT INTO users (name) VALUES (?)', ['张三'])
  if (!inserted.success) throw new Error(inserted.error.message)
  const logged = await tx.execute('INSERT INTO logs (action) VALUES (?)', ['user_created'])
  if (!logged.success) throw new Error(logged.error.message)
  return 'done'
})
// result.data === 'done'（正常返回自动 commit）
// 回调抛异常 → 自动 rollback → result.success === false

// 方式2：手动管理，保留原领域错误（置于 async 函数内）
const txResult = await reldb.tx.begin()
if (!txResult.success) return txResult
const tx = txResult.data
const inserted = await tx.execute('INSERT INTO users (name) VALUES (?)', ['李四'])
if (!inserted.success) {
  const rollback = await tx.rollback()
  if (!rollback.success) return rollback
  return inserted
}
return tx.commit()
```

**事务内可用操作**：

- `tx.query / tx.get / tx.execute / tx.batch / tx.queryPage`
- `tx.crud.table(config)` — 事务内的 CRUD 仓库
- `tx.commit() / tx.rollback()`

### 分页 — `reldb.pagination`

| 方法        | 签名                                                  | 说明                                   |
| ----------- | ----------------------------------------------------- | -------------------------------------- |
| `normalize` | `(options?, overrides?) => NormalizedPagination`      | 规范化分页参数（含 offset/limit 计算） |
| `build`     | `<T>(items, total, pagination) => PaginatedResult<T>` | 构建分页结果对象                       |

```typescript
// PaginatedResult 结构
interface PaginatedResult<T> {
  items: T[]
  total: number
  page: number
  pageSize: number
}

// 默认值：page=1, pageSize=20, maxPageSize=200
```

### JSON — `reldb.json`

通过 `reldb.json` 构建跨数据库统一的 JSON 路径操作 SQL 表达式，返回 `{ sql, params }` 可直接嵌入 `reldb.sql.*`。

支持简单路径 `$.key`、`$.key.subkey`、`$[0]`。

| 操作      | SQLite             | PostgreSQL               | MySQL                  |
| --------- | ------------------ | ------------------------ | ---------------------- |
| `extract` | `json_extract`     | `#>` (text[])            | `JSON_EXTRACT`         |
| `set`     | `json_set`         | `jsonb_set`              | `JSON_SET`             |
| `insert`  | `json_insert`      | `jsonb_insert`           | `JSON_INSERT`          |
| `remove`  | `json_remove`      | `#-` (text[])            | `JSON_REMOVE`          |
| `merge`   | `json_patch`       | 参数化递归 CTE           | `JSON_MERGE_PATCH`     |

`merge` 的补丁根限于可 JSON 序列化对象：三种后端均按 RFC 7396 递归保留未变键、用 null 删除指定键、整体替换数组；PostgreSQL 使用参数化递归 CTE，在同一次 SQL 内更新当前行，不做应用先读再写。路径和值全部参数化，column 只能由开发者静态编写。路径解析只支持简单点号/方括号形式，不是完整 SQL/JSON Path 校验器；PostgreSQL set 要求父路径存在，insert 对已有对象键报错且数组按原生位置插入，不应把不同后端的原生路径行为视为完全相同。

```typescript
import { reldb, type ReldbFunctions } from '@h-ai/reldb'

// db 已初始化，users 表已有 settings/profile JSON 列。
async function updateSettings(db: ReldbFunctions, userId: string) {
  const extracted = db.json.extract('settings', '$.theme')
  const rows = await db.sql.query(`SELECT * FROM users WHERE ${extracted.sql} = ?`, [...extracted.params, '"dark"'])
  if (!rows.success) return rows

  const changed = db.json.set('settings', '$.theme', 'dark')
  const updated = await db.sql.execute(`UPDATE users SET settings = ${changed.sql} WHERE id = ?`, [...changed.params, userId])
  if (!updated.success) return updated

  const removed = db.json.remove('settings', '$.deprecated')
  const deleted = await db.sql.execute(`UPDATE users SET settings = ${removed.sql} WHERE id = ?`, [...removed.params, userId])
  if (!deleted.success) return deleted

  const patch = db.json.merge('profile', { bio: '新简介', avatar: null })
  return db.sql.execute(`UPDATE users SET profile = ${patch.sql} WHERE id = ?`, [...patch.params, userId])
}

const result = await updateSettings(reldb, 'u1')
if (!result.success) throw new Error(result.error.message)
```

> `column` 参数为列名，**禁止传入用户输入**（开发者负责安全性）。

## 错误码 — `HaiReldbError`

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiReldbError.CONNECTION_FAILED`    | `hai:reldb:001`  | 数据库连接失败     |
| `HaiReldbError.QUERY_FAILED`         | `hai:reldb:002`  | 查询或执行失败     |
| `HaiReldbError.CONSTRAINT_VIOLATION` | `hai:reldb:003`  | 约束违反           |
| `HaiReldbError.TRANSACTION_FAILED`   | `hai:reldb:004`  | 事务失败           |
| `HaiReldbError.MIGRATION_FAILED`     | `hai:reldb:005`  | 迁移失败           |
| `HaiReldbError.RECORD_NOT_FOUND`     | `hai:reldb:006`  | 记录不存在         |
| `HaiReldbError.DUPLICATE_ENTRY`      | `hai:reldb:007`  | 重复条目           |
| `HaiReldbError.DEADLOCK`             | `hai:reldb:008`  | 死锁               |
| `HaiReldbError.TIMEOUT`              | `hai:reldb:009`  | 超时               |
| `HaiReldbError.NOT_INITIALIZED`      | `hai:reldb:010`  | 数据库未初始化     |
| `HaiReldbError.POOL_EXHAUSTED`       | `hai:reldb:011`  | 连接池耗尽         |
| `HaiReldbError.UNSUPPORTED_TYPE`     | `hai:reldb:012`  | 不支持的数据库类型 |
| `HaiReldbError.CONFIG_ERROR`         | `hai:reldb:013`  | 配置错误           |
| `HaiReldbError.DDL_FAILED`           | `hai:reldb:020`  | DDL 操作失败       |

## 常见模式

### API 端点中使用

```typescript
import { reldb, HaiReldbError } from '@h-ai/reldb'
import { kit } from '@h-ai/kit'

export const GET = kit.handler(async ({ url }) => {
  // query 返回校验后的数据，失败抛 Response，由 kit.handler 处理。
  const pagination = kit.validate.query(url, PageSchema)
  const result = await userCrud.findPage({ pagination, orderBy: 'id ASC' })
  if (!result.success)
    return kit.response.error(result.error.code, result.error.message)
  return kit.response.ok(result.data)
})
```

### 事务跨仓库

```typescript
const result = await reldb.tx.wrap(async (tx) => {
  const user = await userRepo.create(userData, tx)
  if (!user.success)
    throw new Error(user.error.message)

  const profile = await profileRepo.create({ userId: user.data.lastInsertRowid, ...profileData }, tx)
  if (!profile.success) throw new Error(profile.error.message)
  return user.data
})
```

### BaseReldbCrudRepository 事务集成

与上述事务相同：每个 Repository 方法传入同一 tx，并检查每次 HaiResult；失败 rollback，全部成功后 commit。不要只等待 Promise 完成就提交。

## 相关 Skills

- `hai-build`：模块初始化顺序
- `hai-core`：配置管理、HaiResult 模型
- `hai-iam`：IAM 模块内部使用 reldb 进行用户/角色/权限存储
- `hai-cache`：缓存穿透保护中配合 reldb 使用

## 生产共享状态

生产仅使用 PostgreSQL/MySQL；SQLite（含磁盘文件）只用于开发/明确测试，不能作为跨主机共享状态。 NODE_ENV 或 HAI_CORE_ENV 为 production 时在初始化成功前检查，只有 HAI_E2E=1/HAI_E2E_MOCK=1 明确测试豁免。不要将租约当作最终写入条件，外部未知效果须按稳定业务键核对。

MySQL 空 url（含空环境变量占位）按未配置处理，使用分字段；非空仍严格校验。
