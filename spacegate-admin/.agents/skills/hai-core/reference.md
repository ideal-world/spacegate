# hai-core — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/core` 是 hai-framework 的基础模块，提供配置管理、结构化日志、国际化、HaiResult 错误模型、Zod 校验错误本地化与模块生命周期工具。所有其他模块均依赖 core。

## 运行环境

**Node.js + 浏览器双端可用。** 构建工具自动选择正确入口（`package.json` exports 条件导出）。

| 能力 | Node.js | 浏览器 |
|------|---------|--------|
| `core.logger` | ✅ pino（结构化 JSON / pretty） | ✅ loglevel（DevTools console） |
| `core.config` | ✅ 完整（YAML 加载、watch、validate） | ⚠️ 占位 API；文件配置方法返回 `SERVICE_UNAVAILABLE` |
| `core.i18n` | ✅ | ✅ |
| `core.zodValidation` | ✅ | ✅ |
| `core.id` | ✅ | ✅ |
| 工具函数 | ✅ | ✅ |

## 适用场景

- 项目初始化与配置加载
- 使用 `core.logger` 记录日志
- 使用 `core.config` 读取/校验模块配置
- 使用 `core.i18n` 管理多语言
- 使用 `core.zodValidation` 将 Zod 默认英文错误映射为模块自己的 i18n 文案
- 使用 `ok()` / `err()` 构建 HaiResult 类型返回值
- 创建模块级 logger 或 i18n 消息获取器

## 使用步骤

### 1. 初始化

```typescript
import { core } from '@h-ai/core'

// 自动加载运行时 .yml / .yaml，跳过 _deploy.yml / _deploy.yaml
core.init({ configDir: './config' })

// 也可指定日志级别和配置监听
core.init({ configDir: './config', watchConfig: true })
```

`_deploy.yml` / `_deploy.yaml` 由部署 CLI 在加载目标环境与凭据后独立解析，不参加 Core 的自动加载和监听；业务 `deploy.yml`、手动 `core.config.load` 不受影响。单项 `config.load` 返回失败时记录错误并继续，应用须 validate 必需配置并检查 success。目录扫描、日志设置等异常仍可能传播，不能把同步 void 当作所有配置加载成功的证明。

### 2. 使用功能

```typescript
// 日志
core.logger.info('Server started', { port: 3000 })

// 配置
const dbConfig = core.config.get('db')

// Zod 校验错误本地化
const formErrors = core.zodValidation.mapZodErrorToFormErrors(zodError, getMessage)
```

## 核心 API

### 生命周期

| 方法        | 签名                              | 说明                                      |
| ----------- | --------------------------------- | ----------------------------------------- |
| `core.init` | `(options?: CoreOptions) => void` | 同步加载运行时 YAML，排除两个 `_deploy` 保留文件 |

> `CoreOptions`：`{ logging?: Partial<LoggingConfig>, configDir?: string, watchConfig?: boolean }`
>
> 注意：core 没有 `close()` 方法，初始化后持续可用。

### 配置管理 — `core.config`

> 浏览器端保留 `core.config` 对象用于类型/runtime 对齐，但没有文件系统：`get()` 返回 `undefined`，`has()` 返回 `false`，`keys()` 返回 `[]`；`load()` / `validate()` / `reload()` 返回 `HaiCommonError.SERVICE_UNAVAILABLE`；`watch()` 会立即回调该错误并返回 no-op 取消函数。

| 方法         | 签名                                                           | 说明                           |
| ------------ | -------------------------------------------------------------- | ------------------------------ |
| `get`        | `<T>(name: string) => T \| undefined`                          | 获取模块配置段                 |
| `getOrThrow` | `<T>(name: string) => T`                                       | 获取配置（未加载时抛异常）     |
| `load`       | `<T>(name, filePath, schema?) => HaiResult<T>`       | 手动加载配置文件               |
| `validate`   | `<T>(name: string, schema: ZodType) => HaiResult<T>` | 用 Zod Schema 校验已加载的配置 |
| `has`        | `(name: string) => boolean`                                    | 检查配置段是否存在             |
| `reload`     | `(name: string) => HaiResult<unknown>`               | 重新从磁盘加载配置             |
| `clear`      | `(name?: string) => void`                                      | 清除配置缓存                   |
| `keys`       | `() => string[]`                                               | 获取所有已加载的配置名称       |
| `watch`      | `<T>(name, callback) => () => void`                            | 监听配置文件变更并自动重载     |
| `unwatch`    | `(name?: string) => void`                                      | 停止配置文件监听               |
| `isWatching` | `(name: string) => boolean`                                    | 检查是否正在监听某个配置       |

**`watch()` 回调处理**：

```typescript
// watch 回调签名：(config: T | null, error?: HaiError) => void
// - 成功重载时：传入新配置，error 为 undefined
// - 重载失败时：config 为 null，error 为错误详情（包含 code 和 message）

const unwatch = core.config.watch('db', (cfg, error) => {
  if (error) {
    // 处理重载失败（如文件解析错误），不应更新本地使用的配置
    core.logger.error('Config reload failed', {
      name: 'db',
      code: error.code,
      message: error.message,
    })
    return
  }
  // 使用新配置
  // 注意：此时 core.config.get('db') 已经指向新数据
  core.logger.info('Config updated', { name: 'db' }) // 不记录含密码的完整配置
})

// 调用 unwatch() 停止监听
unwatch()
```

配置文件格式（YAML，支持约定式环境变量映射和显式插值）：

- YAML 根、对象、数组和叶子节点均可映射为 `HAI_<配置名>_<YAML 路径>`；配置名和 key 转大写、层级以 `_` 分隔，camelCase 不拆词
- 例如 `core.config.load('ai', ...)` 加载的 `llm.apiKey` 对应 `HAI_AI_LLM_APIKEY`，`llm.models[0].temperature` 对应 `HAI_AI_LLM_MODELS_0_TEMPERATURE`
- 约定环境变量存在时整体替换对应节点并按 YAML 规则解析；例如 HAI_DB 可注入整段连接配置、HAI_REACH_PROVIDERS 可替换数组。父节点覆盖优先于子节点，覆盖后不再遍历其叶子变量；无父节点覆盖时，只遍历 YAML 已有路径
- 优先级为父节点约定变量 > 当前节点约定变量 > 显式 `${VAR}` > YAML 默认值；显式语法只用于指定特殊变量名

- `${VAR}` — 读取 `process.env.VAR`；缺失则返回 `HaiConfigError.CONFIG_ENV_VAR_MISSING` 错误
- `${VAR:default}` — 读取 `process.env.VAR`；缺失则使用默认值
- **类型还原**：整个值恰好是单个变量表达式时，结果还原为原生类型（number / boolean 等）；混合文本始终为字符串
- **空值语义**：`${VAR:}` 或值为空字符串的 `${VAR}` 保持为 `''`，不会被 YAML 解析为 `null`；是否把空字符串视为未配置由模块 Schema 明确定义

```yaml
# config/_core.yml
name: my-app
env: development
logging:
  level: info
  format: pretty
feature:
  debug: false
  maxRetries: 3
  # 只有特殊变量名才需要显式语法：
  url: http://${HOST}:${PORT} # → string（混合文本，不还原）
```

### 日志 — `core.logger`

`core.logger` 既是默认 Logger 实例，也是日志管理的统一入口。

`core.init` 加载 `_core.yml.logging`、`configure` 和 `setLevel` 会改变已有默认、模块及嵌套 child Logger 的后续实际输出；不需要重新创建模块 Logger。`create` 显式指定的 level/format/context 保持优先，未指定选项动态继承全局配置；调用时 context 覆盖固定 context，附加 redact 规则统一生效。

**日志记录**（实例方法）：`trace` / `debug` / `info` / `warn` / `error` / `fatal`

```typescript
core.logger.info('User created', { userId: '123' })
core.logger.error('Failed to connect', { error })
core.logger.debug('Processing item', { id, step: 'validation' })
```

原生 `Error` 的 `name/message/stack/cause`、`AggregateError.errors` 与可枚举自有上下文先转成副本，再走字段/URL 脱敏与附加 `redact`；两端原异常均不变。Error 子图循环用 `[Circular]`，达到 32 层用 `[MaxDepth]`，自定义替换值也经过同一规范化；普通对象的循环与替换语义不变。自由文本不会自动解析秘密，可配置 `redact: ['error.message', 'error.stack']` 遮蔽整条诊断路径。

**管理方法**：

| 方法        | 签名                                   | 说明                     |
| ----------- | -------------------------------------- | ------------------------ |
| `create`    | `(options?: LoggerOptions) => Logger`  | 创建新 Logger 实例       |
| `child`     | `(context: Record<string, unknown>) => Logger` | 创建携带固定上下文的子 Logger |
| `configure` | `(config: Partial<LoggingConfig>) => void` | 配置全局日志选项（级别、格式等） |
| `setLevel`  | `(level: LogLevel) => void`            | 设置全局日志级别         |
| `getLevel`  | `() => LogLevel`                       | 获取当前全局日志级别     |

```typescript
// 创建模块级 logger
const logger = core.logger.create({ name: 'my-module' })
logger.info('Module initialized')

// 创建携带固定上下文的子 logger（自动合并到每条日志）
const reqLogger = core.logger.child({ requestId: 'req-001' })
reqLogger.info('Request started') // 日志中自动包含 requestId

// 配置输出格式和级别
core.logger.configure({
  level: 'warn',
  format: 'json',
  redact: ['password', 'token'], // 脱敏字段
}) // Node.js 支持
core.logger.setLevel('debug')
const level = core.logger.getLevel() // 'debug'
```

**日志级别规范**：

| 级别    | 适用场景                           |
| ------- | ---------------------------------- |
| `trace` | 循环内详细调试数据                 |
| `debug` | 函数进入、参数概要、读操作         |
| `info`  | 业务事件（初始化完成、写操作成功） |
| `warn`  | 异常但可恢复（校验失败、重试）     |
| `error` | 操作失败（需人工排查）             |
| `fatal` | 致命错误（服务无法继续）           |

### 国际化 — `core.i18n`

| 方法                  | 签名                                                                  | 说明           |
| --------------------- | --------------------------------------------------------------------- | -------------- |
| `getGlobalLocale`     | `() => string`                                                        | 获取当前语言   |
| `setGlobalLocale`     | `(locale: string) => void`                                            | 浏览器语言/启动默认值；请求内只改当前上下文 |
| `runWithLocale`       | `<T>(locale: string, callback: () => T) => T`                           | Node 请求/SSR 异步语言隔离 |
| `createMessageGetter` | `(messages: Record<string, Record<string, string>>) => MessageGetter` | 创建消息获取器 |

创建模块 i18n：

```typescript
import enUS from '../messages/en-US.json'
import jaJP from '../messages/ja-JP.json'
import zhCN from '../messages/zh-CN.json'

const m = core.i18n.createMessageGetter({
  'zh-CN': zhCN,
  'en-US': enUS,
  'ja-JP': jaJP,
})

// 使用
m('user_created') // "用户已创建"
m('welcome', { params: { name: '张三' } }) // "欢迎，张三"
```

### Zod 校验错误映射 — `core.zodValidation`

适用于 `@h-ai/kit`、`@h-ai/serv`、`@h-ai/api-client` 等需要把 Zod 默认英文错误
转换为模块自身 i18n 文案的场景。优先复用 `core.zodValidation`，不要在各模块重复维护
默认消息正则 / issue 本地化逻辑。

| 方法 | 签名 | 说明 |
|------|------|------|
| `createPrefixedZodMessageGetter` | `(prefix, getMessage) => ZodMessageGetter` | 按模块前缀自动派生 `serv_validation*` / `kit_validation*` 这类消息 key |
| `mapZodErrorToFormErrors` | `(error, getMessage) => ValidationFormError[]` | 一步完成提取 + 本地化 + 扁平化 |

```typescript
import type { ZodMessageGetter } from '@h-ai/core'
import { core } from '@h-ai/core'
import { z } from 'zod'

const LoginSchema = z.object({
  email: z.string().email(),
  password: z.string().min(8),
  nickname: z.string().min(1, '请填写昵称'),
})

const getMessage: ZodMessageGetter = (key, params) => {
  switch (key) {
    case 'validationEmail':
      return '请输入合法邮箱地址'
    case 'validationStringMin':
      return `至少输入 ${params?.min} 个字符`
    default:
      return '输入不合法'
  }
}

const result = LoginSchema.safeParse({
  email: 'bad',
  password: '123',
  nickname: '',
})

if (!result.success) {
  const getMessage = core.zodValidation.createPrefixedZodMessageGetter(
    'serv',
    (messageKey, params) => {
      if (messageKey === 'serv_validationEmail')
        return '请输入合法邮箱地址'
      if (messageKey === 'serv_validationStringMin')
        return `至少输入 ${params?.min} 个字符`
      return '输入不合法'
    },
  )
  const errors = core.zodValidation.mapZodErrorToFormErrors(result.error, getMessage)

  errors[0]?.field // 'email'
  errors[0]?.message // '请输入合法邮箱地址'
  errors[2]?.message // '请填写昵称'（schema 自定义消息原样保留）
}
```

### ID 生成 — `core.id`

| 方法           | 签名                                    | 说明                          |
| -------------- | --------------------------------------- | ----------------------------- |
| `generate`     | `(size?: number) => string`             | nanoid（默认 21 字符）        |
| `short`        | `() => string`                          | 10 字符短 ID                  |
| `withPrefix`   | `(prefix: string, length?: number) => string` | 带前缀的 nanoid               |
| `trace`        | `() => string`                          | `trace-` 前缀 ID              |
| `request`      | `() => string`                          | `req-` 前缀 ID                |
| `uuid`         | `() => string`                          | UUID v4                       |
| `isValidUUID`  | `(value: string) => boolean`            | 校验 UUID 格式                |
| `isValidNanoId`| `(value: string, size?: number) => boolean` | 校验 nanoid 格式          |

```typescript
const id = core.id.generate()          // 'V1StGXR_Z5j3eK4uA9b8c'
const shortId = core.id.short()        // 10 字符
const prefixed = core.id.withPrefix('order_') // 'order_V1StGXR...'
const traceId = core.id.trace()        // 'trace-V1StGXR...'
const reqId = core.id.request()        // 'req-V1StGXR...'
const uuid = core.id.uuid()            // UUID v4

core.id.isValidUUID('f47ac10b-...')    // true/false
core.id.isValidNanoId('abc123', 6)     // true/false
```

#### UUID 生成规范（强制）

- 生成 UUID **一律用 `core.id.uuid()`**，禁止裸调 `crypto.randomUUID()`。
- 原因：`crypto.randomUUID` 仅在**安全上下文（HTTPS / localhost）**下可用，生产 http 环境会抛
  `crypto.randomUUID is not a function`；`core.id.uuid()` 仅使用 randomUUID 或密码学安全的 getRandomValues；两者不可用时明确抛错，绝不降级到 Math.random。
- 需要不带横线的 compact 形式：`core.id.uuid().replaceAll('-', '')`。
- 代码评审与新建代码时以此为准，发现裸调 `crypto.randomUUID()` 一律改走 `core.id.uuid()`。

### 工具函数

#### 类型检查 — `core.typeUtils`

`isDefined` / `isObject` / `isFunction` / `isPromise` / `isString` / `isNumber` / `isBoolean` / `isArray`

#### 对象操作 — `core.object`

`deepClone` / `deepMerge` / `pick` / `omit` / `keys` / `values` / `entries` / `fromEntries`

```typescript
core.object.deepMerge(a, b, c)       // 递归合并（防原型污染）
core.object.pick(obj, ['a', 'b'])
core.object.omit(obj, ['c', 'd'])
```

#### 字符串操作 — `core.string`

`capitalize` / `kebabCase` / `camelCase` / `snakeCase` / `pascalCase` / `truncate` / `trim` / `isBlank` / `isNotBlank` / `padStart` / `padEnd` / `constantTimeEqual`

```typescript
core.string.capitalize('hello')      // 'Hello'
core.string.kebabCase('helloWorld')  // 'hello-world'
core.string.constantTimeEqual(a, b)  // Node 使用原生等长比较；浏览器不保证恒时
```

#### 数组操作 — `core.array`

`unique` / `chunk` / `groupBy` / `first` / `last` / `flatten` / `compact` / `shuffle` / `intersection` / `difference`

```typescript
core.array.unique([1, 1, 2])           // [1, 2]
core.array.chunk([1, 2, 3, 4], 2)      // [[1,2], [3,4]]
core.array.groupBy(items, item => item.type) // Record<string, T[]>
core.array.intersection([1, 2], [2, 3]) // [2]
```

#### 异步操作 — `core.async`

`delay` / `withTimeout` / `retry` / `parallel` / `serial` / `debounce` / `throttle`

```typescript
await core.async.delay(1000)
await core.async.withTimeout(fetch('/api'), 5000)
await core.async.retry(fn, { maxRetries: 3, delay: 1000 })
await core.async.parallel(items, processFn, 5)  // 并发限制
const debouncedFn = core.async.debounce(fn, 300)
```

#### 时间操作 — `core.time`

`formatDate` / `timeAgo` / `now` / `nowSeconds` / `parseDate` / `isValidDate` / `addDays` / `addHours` / `startOfDay` / `endOfDay`

```typescript
core.time.formatDate(new Date(), 'YYYY-MM-DD HH:mm:ss')
core.time.timeAgo(new Date(Date.now() - 60000)) // '1 分钟前'
core.time.addDays(date, 7)
```

### HaiResult 模型与错误处理

返回 `HaiResult<T>` 的操作须处理成功/失败分支；core 工具等直接返回值的 API 不套用此模式：

```typescript
import { ok, err, HaiCommonError } from '@h-ai/core'

// 成功
return ok(data)

// 失败
return err(HaiCommonError.NOT_FOUND, 'Resource not found')
```

**使用模式**：

```typescript
const result = await someOperation()

// 模式1：直接检查
if (!result.success) {
  core.logger.error('Operation failed', { error: result.error })
  return result // 直接透传错误
}
const data = result.data

// 模式2：提前返回
if (!result.success)
  return result
// 后续代码可安全使用 result.data
```

## 错误码

Core 模块定义了两组标准错误码：

### `HaiCommonError`（通用错误）

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiCommonError.NOT_INITIALIZED` | `hai:common:001` | 模块未初始化 |
| `HaiCommonError.INIT_FAILED` | `hai:common:002` | 初始化失败 |
| `HaiCommonError.INIT_IN_PROGRESS` | `hai:common:004` | 正在初始化中 |
| `HaiCommonError.UNAUTHORIZED` | `hai:common:100` | 未认证 |
| `HaiCommonError.FORBIDDEN` | `hai:common:101` | 无权限 |
| `HaiCommonError.TOKEN_EXPIRED` | `hai:common:102` | Token 已过期 |
| `HaiCommonError.TOKEN_INVALID` | `hai:common:103` | Token 无效 |
| `HaiCommonError.VALIDATION_ERROR` | `hai:common:200` | 校验失败 |
| `HaiCommonError.INVALID_REQUEST` | `hai:common:201` | 请求无效 |
| `HaiCommonError.PARAMETER_MISSING` | `hai:common:202` | 参数缺失 |
| `HaiCommonError.NOT_FOUND` | `hai:common:300` | 资源不存在 |
| `HaiCommonError.ALREADY_EXISTS` | `hai:common:301` | 资源已存在 |
| `HaiCommonError.CONFLICT` | `hai:common:302` | 冲突 |
| `HaiCommonError.API_ERROR` | `hai:common:400` | API 错误 |
| `HaiCommonError.NETWORK_ERROR` | `hai:common:401` | 网络错误 |
| `HaiCommonError.TIMEOUT` | `hai:common:402` | 超时 |
| `HaiCommonError.SERVICE_UNAVAILABLE` | `hai:common:403` | 服务不可用 |
| `HaiCommonError.INTERNAL_ERROR` | `hai:common:500` | 内部错误 |
| `HaiCommonError.DATABASE_ERROR` | `hai:common:501` | 数据库错误 |
| `HaiCommonError.UNKNOWN_ERROR` | `hai:common:599` | 未知错误 |

**使用示例**：

```typescript
import { HaiCommonError, err } from '@h-ai/core'

// 创建错误实例
err(HaiCommonError.NOT_FOUND, 'User not found')
err(HaiCommonError.VALIDATION_ERROR, 'Invalid email', validationErrors)
```

### `HaiConfigError`（配置错误）

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiConfigError.CONFIG_FILE_NOT_FOUND` | `hai:core:010` | 配置文件不存在 |
| `HaiConfigError.CONFIG_PARSE_ERROR` | `hai:core:011` | YAML 解析失败 |
| `HaiConfigError.CONFIG_VALIDATION_ERROR` | `hai:core:012` | Schema 校验失败 |
| `HaiConfigError.CONFIG_ENV_VAR_MISSING` | `hai:core:013` | 必需环境变量缺失 |
| `HaiConfigError.CONFIG_NOT_LOADED` | `hai:core:014` | 配置未加载 |

**使用场景**：

```typescript
import { core, HaiConfigError } from '@h-ai/core'

const result = core.config.load('db', './config/db.yml', schema)
if (!result.success) {
  if (result.error.code === HaiConfigError.CONFIG_FILE_NOT_FOUND.code) {
    core.logger.error('Config file missing')
  }
  else if (result.error.code === HaiConfigError.CONFIG_VALIDATION_ERROR.code) {
    core.logger.error('Config validation failed', { issues: result.error.cause })
  }
}
```

## 常见模式

### 服务端初始化（SvelteKit hooks.server.ts）

在应用已有的 init.ts 中执行 core.init({ configDir: './config' })，然后初始化依赖模块；hooks.server.ts 等待该启动流程完成后接入 kit.createHandle。完整依赖顺序与失败清理见 [hai-build](../hai-build/SKILL.md)。不要在一个文件里同时 import 和声明同名 initModules。

### 配置校验最佳实践

```typescript
import { z } from 'zod'

const MyConfigSchema = z.object({
  apiKey: z.string().min(1),
  timeout: z.number().positive().default(5000),
})

// 模块初始化时校验
const result = core.config.validate('myModule', MyConfigSchema)
if (!result.success) {
  core.logger.error('Invalid config', { error: result.error })
  return result
}
const config = result.data // 类型安全
```

## 相关 Skills

- `hai-build`：项目整体架构与技能导航
- `hai-kit`：SvelteKit 集成层（Handle/Guard/Middleware）
- `hai-reldb` / `hai-cache` / `hai-iam`：使用 core.config 读取配置的下游模块

## 浏览器侧使用

> **禁止在应用中直接引入 `loglevel`。** `core.logger` 在浏览器端自动使用 loglevel 作为后端，API 与 Node.js 完全一致。

### 初始化

浏览器端无文件系统，不需要 `configDir`。在 `+layout.svelte` 或应用入口调用：

```typescript
import { core } from '@h-ai/core'

// 浏览器端：仅传入 logging 配置即可
core.init({ logging: { level: 'info' } })
```

### 日志记录

```typescript
// 与 Node.js 完全一致的 API
core.logger.info('Page loaded', { route: '/home' })
core.logger.error('API call failed', { url, status })

// 创建模块级 logger
const logger = core.logger.create({ name: 'my-page' })
logger.debug('Data fetched', { count: items.length })

// ❌ 禁止直接引用底层日志库
// import log from 'loglevel'  ← 不要这样做
```

### 浏览器侧限制

| 功能 | 行为 |
|------|------|
| `core.config` 文件配置 | 不支持；文件相关方法返回 `SERVICE_UNAVAILABLE`，配置应由服务端读取后下发 |
| `format: 'json' \| 'pretty'` | 始终输出到 DevTools console；json 为包含 level/msg 的 JSON 字符串，pretty 为可读消息与上下文 |
| 自定义 `redact` 配置 | 与默认 sanitizeSensitiveFields 同时生效；支持嵌套点/括号路径与 `*`，不修改调用方对象；消息文本需调用方脱敏 |
| `fatal` 级别 | 映射为 console.error；pretty 带 `[FATAL]` 前缀，json 使用 `level: 'fatal'` |
| context 输出 | pretty 追加 JSON 上下文，json 合并上下文到结构化 JSON 字符串 |

云资源输出格式与部署配置消费见 [hai-deploy](../hai-deploy/SKILL.md)；这里的父节点覆盖规则同样适用。

### URL 日志脱敏

使用 `core.sanitize.sanitizeUrl(url)` 统一处理连接地址；它会遮蔽用户名、密码、敏感 query 参数和 OAuth 键值 fragment。非法地址返回固定占位符。`sanitizeSensitiveFields` 对 URL 类字段使用同一规则；自定义 matcher 同样匹配参数名，不能只遮蔽 userinfo 后记录含 token 的 URL。

```ts
const safeUrl = core.sanitize.sanitizeUrl("https://example.com/a?token=value&limit=10")
// https://example.com/a?token=[REDACTED]&limit=10
```

Node 请求/SSR 必须用 `core.i18n.runWithLocale(locale, callback)`（kit 同名方法）包裹实际异步 resolve；serv 自动建立请求语言上下文。浏览器 setGlobalLocale/createLocaleStore 继续管理应用语言，SSR 的 UI setter/store 不写全局值。显式 MessageOptions.locale 优先；进程默认语言仅在启动时设置。

Node 入口的 core.string.constantTimeEqual 将原始 UTF-16 单元编码后使用 node:crypto.timingSafeEqual 比较等长字节，长度不相同时直接 false，长度本身不受保护。浏览器实现仅避免按内容提前返回，不提供 JavaScript 引擎级恒时保证，不用于服务端凭证校验。外围分支、编码和请求处理不因此自动成为恒时。
