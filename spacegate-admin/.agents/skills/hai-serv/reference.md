# hai-serv — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/serv` 是 hai-framework 的 API Service 运行时，对外暴露最小 HTTP App 抽象，将 `@h-ai/api-contract` 的领域 contract 挂载成跨端可访问的 HTTP API；Hono 是内部实现细节，应用代码不要直接 import 或暴露 Hono。

## 运行环境

> ⚠️ **服务端模块（Node.js 或满足同等 Node API 依赖的宿主）。** 浏览器端通过 `@h-ai/api-client` 调用由本模块暴露的 HTTP API。

> 生命周期说明：`@h-ai/serv` 是无状态 HTTP App 装配器，不提供 `init()` / `close()`；依赖模块（iam/storage/ai/crypto 等）由应用先初始化，并在 `serv.listen(..., { onClose })` 中反向释放。`serv.createApp()` 的启动期配置错误会 fail-fast 抛出。

## 适用场景

- 将 `@h-ai/api-contract` contract 装配成最小 HTTP App 抽象
- 配置 `/health`、`/ready`、`/openapi.json`、`/docs` 等系统端点
- 在 procedure route 上添加 `.auth()`、`.permission()` 或 `.role()` guard
- 使用 Node listener，或在满足包 Node 依赖的宿主/测试中注入标准 fetch handler
- 自定义 `ServContext`（注入 session、tenant 等请求级数据）
- 生成 OpenAPI 3.1 文档供外部工具消费
- 通过 `config/_serv.yml` 管理 API 前缀、OpenAPI、docs、health、rpc 等 HTTP 挂载配置
- 通过 `serv.createRuntimeSecurityPolicy(...)` 对生产 CORS Origin、原生 WebView Origin、Secure Cookie 与 API 文档暴露执行 fail-closed 策略
- 启用传输加密：`serv.createApp({ transport: { crypto, serverKeyRing, keyStore } })`
- 高级场景通过 `createApp({ middlewares })` 自定义 HTTP middleware，通过 `createContext` 注入请求级业务字段
- 公开只读文件通过 `serv.storageAssets(...)` 统一处理安全 key、MIME 白名单、GET/HEAD、ETag 与缓存头
- 为 Audio WebSocket 配置一次性 ticket 校验、IAM/模型/配额授权与并发释放钩子

## 使用步骤

### 1. 安装依赖

```typescript
// @h-ai/serv 依赖 @h-ai/api-contract 提供 contract 定义
// 业务 procedure 依赖对应领域模块（iam/storage/ai）
```

### 2. 创建并启动 app

```typescript
import { ai } from '@h-ai/ai'
import { apiContract } from '@h-ai/api-contract'
import { iam } from '@h-ai/iam'
import { serv } from '@h-ai/serv'
import { createAiProcedures } from '@h-ai/serv/features/ai'
import { createIamProcedures } from '@h-ai/serv/features/iam'
import { createStorageProcedures } from '@h-ai/serv/features/storage'
import { storage } from '@h-ai/storage'

const contract = apiContract.create({ iam: apiContract.iam, storage: apiContract.storage, ai: apiContract.ai })

// 装配 procedures（每个 feature 对应 contract 中的一个领域）
const procedures = {
  iam: createIamProcedures({ iam }),
  storage: createStorageProcedures({
    storage,
    authorize: ({ context, keys }) => keys.every(key =>
      key.startsWith(`users/${encodeURIComponent(context.session.userId)}/`)),
  }),
  ai: createAiProcedures({ ai }),
}

// 创建 HTTP App 抽象；不要在应用导出类型里暴露 Hono
const app = serv.createApp({
  contract,
  procedures,
  http: {
    apiPrefix: '/api/v1',
    openapi: { path: '/openapi.json' },
    docs: { path: '/docs' },
    health: { path: '/health', readyPath: '/ready' },
  },
})

// Node.js 启动（host/port 来自 _serv.yml 的 server；HOST/PORT 环境变量高于配置文件；onClose 自动监听 SIGINT/SIGTERM）
const { server } = core.config.getOrThrow<ServConfig>('serv')
serv.listen(app, {
  host: server.host,
  port: server.port,
  onListening: info => logger.info('API service listening', { port: info.port }),
  onClose: closeApp, // 业务模块关闭函数（ai/storage/iam/reldb 等反向释放）
})
```

生产部署先从 `_core.yml.env` 与 `_serv.yml.cors` 创建安全策略；不要在业务应用重复实现 Origin 解析：

```typescript
const security = serv.createRuntimeSecurityPolicy({
  environment: coreConfig.env,
  corsOrigin: servConfig.cors.origin,
  nativeOrigins: servConfig.cors.nativeOrigins,
})

const app = serv.createApp({
  contract,
  procedures,
  http: {
    ...servConfig.http,
    openapi: security.exposeApiDocs ? servConfig.http.openapi : false,
    docs: security.exposeApiDocs ? servConfig.http.docs : false,
  },
  refreshCookie: { secure: security.secureRefreshCookie },
  middlewares: [{
    middleware: serv.cors({
      origin: security.allowOrigin,
      credentials: servConfig.cors.credentials,
      allowedHeaders: [...servConfig.cors.allowedHeaders],
      exposedHeaders: [...servConfig.cors.exposedHeaders],
    }),
  }],
})
```

- 生产环境 `cors.origin` 禁止为空或 `*`，启动时 fail-fast。
- 普通 Web Origin 仅接受 HTTP(S)；原生 Origin 通过 `cors.nativeOrigins` 显式允许 HTTP(S)、Capacitor 或 Tauri。
- Origin 按完整规范化值精确匹配，不使用前缀/后缀匹配。
- 生产环境自动要求 Secure refresh cookie，并把 OpenAPI/docs 作为安全上限关闭。

### 2.1 Audio WebSocket 安全接入

```typescript
const app = serv.createApp({
  contract,
  procedures,
  iam,
  audio: {
    ai,
    verifyTicket: consumeAudioTicket, // 校验用途/时效并原子消费，返回 { session, grant? }（推荐 iam.ticket.consume）
    authorize: (session, request, grant) => authorizeAudioRequest(session, request, grant),
    onSessionEnd: (session, request) => releaseAudioConcurrency(session.userId, request.operation),
  },
})
```

- ticket 由已登录 HTTP 请求签发（推荐 `iam.ticket.issue`），短期有效且只能消费一次；连接建立即鉴权（独立预鉴权超时，默认 5s），未鉴权不处理任何业务帧；普通 IAM access token 禁止进入 WebSocket URL。
- `verifyTicket` 返回 `HaiResult<AudioTicketVerification>`（`{ session, grant? }`）；`grant` 承载票据绑定的操作/模型/会话，供 `authorize` 交叉校验 `start` 未越权。
- 每帧经运行时 Zod 校验并按严格状态机接受（识别只收音频、合成只收文本、`start`/`done` 各一次、`segmentId` 会话内唯一）；输入队列、消息积压、发送缓冲均有上限（`preAuthTimeoutMs`/`maxPendingMessages`/`maxSendBufferBytes` 等可调），超限或取消时以领域错误关闭并级联中止上游。
- `authorize` 返回的 `AuthorizedAudioRequest` 是唯一会传给 `ai.audio` 的模型、音色与格式配置；未提供时客户端的付费参数会被忽略（若票据 `grant.model` 存在则采用之）。
- IAM 权限、Persona、套餐配额和并发计数属于应用策略；框架提供校验与结束钩子，不硬编码业务权限名或存储。

### 3. 扩展应用自有 contract + procedures

`apiContract.create({...})` 接受任意 key——除了框架提供的 `iam/storage/ai`，应用可以挂入自己的领域 contract，
客户端通过 `client.<key>.<procedure>(...)` 调用，类型完全推导自同一份 contract。

```typescript
// apps/api-service-contract/src/app-contract.ts
import { apiContract } from '@h-ai/api-contract'
import { z } from 'zod'

const InfoOutput = apiContract.haiResultSchema(z.object({ name: z.string(), version: z.string() }))
const EchoInput = z.object({ message: z.string().min(1).max(2000) })
const EchoOutput = apiContract.haiResultSchema(z.object({ message: z.string(), userId: z.string() }))

export const appContract = {
  info: apiContract.route({ method: 'POST', path: '/app/info', operationId: 'app.info', tags: ['app'] })
    .output(InfoOutput),
  echo: apiContract.route({ method: 'POST', path: '/app/echo', operationId: 'app.echo', tags: ['app'] })
    .input(EchoInput)
    .output(EchoOutput),
}
```

```typescript
// src/server/procedures/app-procedures.ts
import type { ServContext } from '@h-ai/serv'
import { appContract } from '@h-ai/api-service-contract'
import { ok } from '@h-ai/core'
import { serv } from '@h-ai/serv'

export function createAppProcedures(deps: { name: string, version: string }) {
  return serv
    .implement(appContract)
    .context<ServContext>()
    // 公开 procedure：route 名只出现一次，handler 直接返回 HaiResult。
    .route('info', () => ok({ name: deps.name, version: deps.version }))
    // 鉴权 procedure：guard 通过后 context.session 自动收窄为非空。
    .route('echo')
    .auth()
    .handle(({ input, context }) => ok({
      message: input.message,
      userId: context.session.userId,
    }))
    .build()
}
```

把自有 contract / procedures 与框架的 iam/storage/ai 平铺合并：

```typescript
const contract = apiContract.create({
  iam: apiContract.iam,
  storage: apiContract.storage,
  ai: apiContract.ai,
  app: appContract, // ← 自有契约
})

const procedures = {
  iam: createIamProcedures({ iam }),
  storage: createStorageProcedures({
    storage,
    authorize: ({ context, keys }) => keys.every(key =>
      key.startsWith(`users/${encodeURIComponent(context.session.userId)}/`)),
  }),
  ai: createAiProcedures({ ai }),
  app: createAppProcedures({ name: 'demo', version: '1.0.0' }),
}

const app = serv.createApp({ contract, procedures, http, iam })
```

注意要点：

- **无 input 的 POST 过程**：业务层无需 input，但 transport 仍发送空明文的认证封装。GET/HEAD 使用 X-Hai-Transport 头；空请求与响应不存在未认证旁路。
- **认证授权写在 route 链上**：公开过程使用 `.route(path, handler)`；需要认证时使用 `.route(path).auth().handle(handler)`。
- **客户端导入相同 contract**：把服务端与客户端共享的 contract 独立成 package（如 `@h-ai/api-service-contract`），禁止从 app 源码目录跨应用 import。

### 5. 标准 fetch 签名适配

toFetch 只适配调用签名；包根仍静态依赖 Node API，未经实际验证不能据此宣称 Workers/Deno/纯边缘部署兼容。

```typescript
import { serv } from '@h-ai/serv'

// toFetch 返回标准 Fetch handler
const handler = serv.toFetch(app)

export default { fetch: handler }
```

### 6. 通过 `config/_serv.yml` 管理 HTTP 配置

`@h-ai/serv` 不自己扫描 YAML；推荐由 `@h-ai/core` 统一加载，再用 `ServConfigSchema` 校验：

```typescript
import { core } from '@h-ai/core'
import { serv, ServConfigSchema } from '@h-ai/serv'

core.init({ configDir: './config' })

const validation = core.config.validate('serv', ServConfigSchema)
if (!validation.success)
  throw new Error(validation.error.message)

const servConfig = core.config.getOrThrow<import('@h-ai/serv').ServConfig>('serv')

const app = serv.createApp({
  contract,
  procedures,
  http: servConfig.http,
  transport: servConfig.transport === false
    ? undefined
    : {
        crypto,
        serverKeyRing,
        keyStore,
        keyExchangePath: servConfig.transport.keyExchangePath,
        excludePaths: [...servConfig.transport.excludePaths],
      },
})
```

```yaml
# config/_serv.yml
server:
  host: 127.0.0.1
  port: 3000

http:
  trustedProxies: [] # 仅信任会清洗/追加转发头的代理 IP/CIDR
  apiPrefix: /api/v1
  openapi:
    path: /openapi.json
  docs:
    path: /docs
  health:
    path: /health
    readyPath: /ready
  rpc: false

transport:
  keyExchangePath: /_hai/key-exchange
  excludePaths:
    - /health
    - /ready
    - /openapi.json
    - /docs
    - /_hai/scalar.js
```

认证协议仅支持 v2：独立随机 SM4-CBC 密钥和 HMAC-SM3 密钥由 SM2 封装，先验签、再验证 MAC，最后解密 CBC。请求由客户端私钥签名，响应由所选服务端版本签名。认证字段包括 version、direction、serverKeyId、clientId、128 位随机 requestId（兼 nonce）、issuedAt、HTTP method、pathname + 原始 search 的 SM3 pathHash，以及响应 status。请求有效期 5 分钟，未来时钟偏差最多 30 秒；共享 registry 原子消费 (clientId, requestId)，nonce 至少保留到 issuedAt + 6 分钟以覆盖节点偏差，存储故障拒绝请求。Redis 必须保留有效 nonce，使用 noeviction/可靠持久化策略；各节点时钟须同步。此能力防止同一传输消息重放，业务重试仍需自己的幂等键。

空 POST 也发送空明文的认证封装；GET/HEAD 将封装放在 X-Hai-Transport 头，204/304/HEAD 等无正文响应也使用同名头承载认证封装。其它请求/响应在 JSON 正文承载封装并带 X-Encrypted: true。客户端必须关联原请求上下文和 HTTP status，缺标记、旧格式或认证失败一律 reject；明文接口只能经 excludePaths 显式排除。CORS 须允许并暴露 X-Hai-Transport。握手公钥的真实性依赖 TLS，不替代 IAM 业务授权。

## 核心 API

### `serv.createApp(options)` — 创建 HTTP App 抽象

```ts
import type { CreateServAppOptions, ServContext } from '@h-ai/serv'
import { apiContract } from '@h-ai/api-contract'
import { serv } from '@h-ai/serv'

// 最小服务使用默认健康检查；业务路由由真实 contract/procedures 扩展。
const contract = apiContract.create({})
const procedures = serv.implement(contract).context<ServContext>().build()
const options: CreateServAppOptions<typeof contract, typeof procedures> = {
  contract,
  procedures,
  http: { apiPrefix: '/api/v1' },
}
const app = serv.createApp(options)
```

### `apiContract.route` / `serv.implement` — contract 与运行时分层

应用代码不要直接 `import { oc } from '@orpc/contract'` 或 `import { implement } from '@orpc/server'`。
contract 路由统一用 `@h-ai/api-contract` 定义，procedure 运行时统一用 `@h-ai/serv` 实现：

```typescript
import { apiContract } from '@h-ai/api-contract'
import { serv } from '@h-ai/serv'

// 等价于 oc.route(...)，但不让应用感知 @orpc/contract
const route = apiContract.route({ method: 'POST', path: '/x', operationId: 'x', tags: ['x'] })

const procedures = serv
  .implement(appContract)
  .context<ServContext>()
  .route('info', infoHandler)
  .route('echo')
  .auth()
  .handle(echoHandler)
  .build()
```

### 传输加密

serv 不暴露本地传输加密工厂；内部统一调用 `crypto.transport.createServer({ serverKeyRing, keyStore })`。客户端用 `@h-ai/api-client` 的 `transport` 配置自动协商。

正文统一使用 `@h-ai/crypto` 根入口的 `TRANSPORT_BODY_LIMITS` / `readTransportBody` 有界读取原始流：握手 JSON 16 KiB、公钥 4 KiB、明文 1 MiB、密文 JSON 2 MiB + 32 KiB，均按 UTF-8 字节计数。无 Content-Length 时也逐块检查，超限或取消立即停止、取消原流并释放 reader，不 clone/tee 后完整缓冲。请求超限返回 413，响应超限返回 500，无效公钥返回 400。明文 Request 保留 signal 并清空 Hono 正文缓存；重建请求/响应时移除旧 Content-Length/Content-Encoding。预算为固定协议常量，不属于 `_serv.yml` 或环境变量。

```typescript
import { crypto } from '@h-ai/crypto'
import { serv } from '@h-ai/serv'

await crypto.init()

const app = serv.createApp({
  contract,
  procedures,
  http: { apiPrefix: '/api/v1' },
  transport: {
    crypto,
    serverKeyRing,
    // keyExchangePath 默认 '/_hai/key-exchange'
    keyStore, // 已连接的共享 Redis/数据库 registry。
  },
})
```

客户端：

```typescript
await apiClient.init({
  baseUrl: 'https://api.example.com/api/v1',
  transport: { crypto },
})
```

若服务端通过 `config/_serv.yml` 自定义了 `transport.keyExchangePath`，客户端也必须传入同一路径：

```typescript
await apiClient.init({
  baseUrl: 'https://api.example.com/api/v1',
  transport: {
    crypto,
    keyExchangePath: '/custom/key-exchange',
  },
})
```

> 不要导入或暴露子目录内部 transport 工厂；公共装配点只有 `serv.createApp({ transport: { crypto, serverKeyRing, keyStore } })`。

启用 transport 必须显式注入 `serverKeyRing` 与 `keyStore`；多节点使用相同受管 ring 和共享 Redis/数据库 registry，仅共享客户端公钥不足以跨节点解密。通过根入口 `parseTransportServerKeyRing(process.env.HAI_CRYPTO_TRANSPORT_SERVERKEYRING)` 解析并检查 HaiResult，私钥不放 `_serv.yml`、PUBLIC_/VITE_、日志或响应。请求带协商 serverKeyId，未知/撤销 ID 拒绝。轮换先分发新旧 ring，再切 activeKeyId 并滚动重建；删除旧版本后客户端须 destroy 并重协商，不自动重放非幂等业务。默认 YAML maxClients 已删除；开发/测试显式 `createInMemoryKeyStore(capacity)`。无需 sticky session，详见 hai-crypto 的密钥轮换说明。

**上下文工厂优先级**（`context.session` 填充来源）：

1. 显式 `createContext`（高级场景：多租户、额外字段）
2. `verifyToken`（逃脱口：使用非 @h-ai/iam 的认证服务）
3. `iam.auth.verifyToken`（推荐：传入 `iam` 后自动启用）
4. 默认 `parseRequestContext`：仅解析请求元数据，`session` 为 `undefined`

> ⚠️ `verifyToken` 会在每次请求调用（不缓存）；verifyToken 失败或抛错统一收敛为 `session=undefined`，由 route 的 `.auth()` / `.permission()` / `.role()` 返回失败 HaiResult，`error.httpStatus=401`；普通 procedure 的 HTTP 响应默认仍为 200。

### 链式 procedure guard

| 链式调用                                             | 说明                                                                                     |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `.route(path, handler)`                              | 注册公开 procedure；path/input/output 从 contract 推导                                   |
| `.route(path).auth().handle(handler)`                | 要求 session；通过后 `context.session` 自动收窄                                          |
| `.permission(permission)`                            | 隐含认证并检查权限，支持 `serv.WILDCARD_PERMISSION`                                      |
| `.role(role)`                                        | 隐含认证并检查角色，支持 `serv.WILDCARD_ROLE`                                            |
| `.build()`                                           | contract 全部 procedures 实现后构建 router                                               |
| `serv.validateInputOrFail(zodSchema, input, locale)` | 在 procedure 内执行 Zod 二次校验，失败时返回本地化 `HaiResult` + `ValidationFormError[]` |
| `serv.resolveRequestLocale(headers)`                 | 从 `x-hai-locale` / `Accept-Language` 解析并规范化 locale                                |
| `serv.m(key, { locale, params })`                    | 读取 serv 自身 i18n 消息（请求级本地化）                                                 |

```typescript
import { serv } from '@h-ai/serv'

const procedures = serv
  .implement(appContract)
  .context<ServContext>()
  .route('info', infoHandler)
  .route('users.update')
  .permission('user.write')
  .role('admin')
  .handle(updateUserHandler)
  .build()
```

- `.permission()` / `.role()` 隐含 `.auth()`；未登录返回 `error.httpStatus=401` 的失败 HaiResult，授权不足为 403。普通 procedure 的 HTTP 响应默认仍为 200，必须检查 `success` 与 `error.code`。
- 同一路由声明多个 permission/role 时必须全部满足。
- handler 的未处理异常由 router 统一转换成 INTERNAL_ERROR HaiResult。
- 重复/未知路径会被拒绝；遗漏 contract procedure 时 `.build()` 在类型上不可用，运行时也会校验。

### 高级：自定义 pipeline（HTTP middleware + context + route guard）

`@h-ai/serv` 的 pipeline 分三层：

1. **HTTP middleware 层**：通过 `serv.createApp({ middlewares })` 注入 HTTP middleware
2. **context 层**：通过 `verifyToken` / `createContext` / `serv.buildAuthContextFactory()` 自定义请求上下文
3. **route guard 层**：通过 `.auth()` / `.permission()` / `.role()` 声明认证授权

#### HTTP middleware

适用于请求日志、trace、限流、CORS、租户头校验等 **HTTP 层** 横切逻辑，也可挂载不适合进入 JSON/oRPC contract 的 WebSocket、文件和二进制端点。常见 CORS 场景可直接复用 `serv.cors(...)`：

```typescript
import type { ServMiddleware } from '@h-ai/serv'
import { serv } from '@h-ai/serv'

const requestMetrics: ServMiddleware = async (c, next) => {
  const startedAt = Date.now()
  await next()
  c.header('x-response-time-ms', String(Date.now() - startedAt))
}

const app = serv.createApp({
  contract,
  procedures,
  http: { apiPrefix: '/api/v1' },
  middlewares: [
    {
      middleware: serv.cors({
        origin: origin => origin === 'https://app.example.com',
        credentials: true,
        exposedHeaders: ['X-Encrypted', 'X-Request-Id'],
      }),
    },
    { middleware: requestMetrics },
    {
      path: '/api/v1/*',
      middleware: async (c, next) => {
        if (!c.req.header('x-tenant-id'))
          return c.text('Missing x-tenant-id', 400)
        await next()
      },
    },
    {
      path: '/assets/*',
      middleware: serv.storageAssets({
        storage,
        pathPrefix: '/assets/',
        keyPattern: /^public\/[\w-]+\.png$/,
        allowedContentTypes: ['image/png'],
        cacheControl: 'public, max-age=31536000, immutable',
        crossOriginResourcePolicy: 'cross-origin',
      }),
    },
  ],
})
```

执行顺序固定为：`securityHeaders` → `middlewares` → `transport`（若启用）→ health/refresh-cookie/OpenAPI/RPC/docs/oRPC routes。

- `middlewares` 先于 `transport` 执行，因此 CORS preflight 或二进制端点可以直接短路；短路响应仍带内置安全头，但不会经过 transport。
- 公开只读文件优先使用 `serv.storageAssets(...)`；应用仍须显式提供 key/MIME 白名单、缓存与跨域策略，框架不内置头像等业务概念。
- 私有资源、Range 下载或动态授权继续使用自定义 middleware/procedure。
- 若需要读取解密后的业务 body，请改用 context / procedure 层扩展。
- 浏览器若需读取自定义响应头（例如 transport 的 `X-Encrypted`），记得通过 `serv.cors({ exposedHeaders: [...] })` 显式暴露。

#### Context pipeline

如果想复用“Bearer token → session”这段逻辑，再追加租户/工作区等字段，优先：

```typescript
const baseContext = serv.buildAuthContextFactory(token => iam.auth.verifyToken(token))

const app = serv.createApp({
  contract,
  procedures,
  createContext: async (input) => {
    const context = await baseContext(input)
    return {
      ...context,
      tenantId: input.request.headers.get('x-tenant-id') ?? null,
    }
  },
})
```

选择建议：

- 只换 token 校验实现：`verifyToken`
- 在默认认证上下文之上追加字段：`buildAuthContextFactory(...) + createContext`
- 完全接管上下文构造：`createContext`

#### Route guard

认证、权限与角色约束直接写在目标 route 上，不嵌套高阶 handler：

```typescript
const procedures = serv
  .implement(widgetContract)
  .context<ServContext>()
  .route('widgets.list', ({ input }) => widgetService.list(input))
  .route('widgets.create')
  .permission('widgets.write')
  .handle(({ input, context }) =>
    widgetService.create(context.session.userId, input))
  .build()
```

HTTP 审计、限流、CORS、租户头校验等跨 route 能力继续放在
`createApp({ middlewares })`；请求级业务字段通过 `createContext` 注入。
不要为认证授权重新嵌套多层高阶 handler。

### 正常业务请求中的错误与 i18n

先区分 HTTP 状态与 HaiResult 中的 `error.httpStatus`；普通 procedure 返回的领域错误元数据不会自动改写 HTTP 状态，当前默认装配如下（显式自定义响应状态或 middleware 按实际入口契约验证）：

| 失败来源                                           | HTTP 响应 | 结果判定                                                                  |
| -------------------------------------------------- | --------- | ------------------------------------------------------------------------- |
| procedure `.auth()` / `.permission()` / `.role()`  | 200       | `success=false`；`error.httpStatus` 为 401 / 403                          |
| handler 返回 `err(...)` 或下游模块失败 HaiResult   | 200       | 检查 `success`、`error.code`，例如数据库查询错误的 `error.httpStatus=500` |
| handler 未处理异常被 route 实现器捕获              | 200       | INTERNAL_ERROR 失败 HaiResult，`error.httpStatus=500`                     |
| oRPC contract 输入 schema 校验失败（handler 之前） | 400       | 本地化失败体，`error.httpStatus=400` 与字段错误                           |
| handler 内 `serv.validateInputOrFail(...)` 失败    | 200       | 失败 HaiResult，`error.httpStatus=400` 与字段错误                         |

HTTP middleware、自定义 Response、refresh-cookie、OpenAPI/RPC 入口权限和 transport 协议错误分别按自身响应契约处理，可返回真实 HTTP 4xx/5xx。`@h-ai/api-client` 在真实 HTTP 401 或 HTTP 200 的有效失败 HaiResult（`success=false`、字符串 `error.code/message`、`error.httpStatus=401`）后自动刷新，成功后只重试一次；内置登录、注册和配置的刷新路径除外，403/500 不刷新。启用 transport 时刷新和重试复用加密链路，不能手写明文刷新请求。

SvelteKit 的 `kit.response.fromError` 根据 `error.httpStatus` 生成 HTTP Response；`kit.response.fromResult` 使用显式错误码映射，未命中默认 400。这些响应转换规则不能套用到 serv 普通 procedure。

按三层处理：

1. **oRPC contract 输入校验**：进入 handler 前自动发生；`serv.createApp()` 会把默认英文 Zod 错误改写为本地化 `errors[]`。
2. **handler 内二次校验**：对跨字段规则、数据库回读对象等适合 Zod 表达的场景，使用 `serv.validateInputOrFail(zodSchema, input, context.locale)`；简单业务规则直接 `err(...)` 即可。
3. **业务/领域错误**：不要 throw 预期失败；直接返回 `err(...)`，并在**创建错误消息的那一层**用对应模块的 i18n getter 按 `context.locale` 出消息。

```typescript
import { serv } from '@h-ai/serv'
import { z } from 'zod'

const Schema = z.object({ title: z.string().min(1) })

const procedures = serv
  .implement(widgetContract)
  .context<ServContext>()
  .route('widgets.create')
  .auth()
  .handle(({ input, context }) => {
    const validated = serv.validateInputOrFail(Schema, input, context.locale)
    if (!validated.success)
      return validated

    return widgetService.create(context.session.userId, validated.data)
  })
  .build()
```

> 如果错误来自下游模块（如 `iam` / `storage`）并且该模块已经返回 `HaiResult`，serv 默认**透传**它的 `error.message`。由于 `HaiError` 目前不携带 `messageKey/params`，serv 边界不能对任意下游错误再做通用重翻译；要做请求级 i18n，必须在创建该错误的模块/feature 里就拿到 locale。

### `serv.parseRequestContext` — 默认 Context 解析器

默认忽略转发头，缺少运行时对端时 IP 为 undefined。XFF 从右向左停在首个不可信地址，忽略左侧；可信链无效/过长时保留 IP 未知，不能回退为代理身份。Fetch 使用 `app.fetch(request, { remoteAddress: platformPeerAddress })`，对端必须来自可信平台 API。自定义 context 传递完整 input，保留已解析的 ip；内部 RPC 使用连接对端，转发头不能放宽来源限制。

从 HTTP 请求头自动解析：

| 字段          | 来源                                                                                                      |
| ------------- | --------------------------------------------------------------------------------------------------------- |
| `accessToken` | `Authorization: Bearer <token>`                                                                           |
| `requestId`   | `x-request-id` 或自动生成 UUID                                                                            |
| `ip`          | Node socket / Fetch 显式 remoteAddress；仅 http.trustedProxies 中的代理可提供 XFF（从右向左）或 X-Real-IP |
| `locale`      | `accept-language`                                                                                         |
| `userAgent`   | `user-agent`                                                                                              |

### `serv.generateSpec(contract, options)` — 生成 OpenAPI 文档

```typescript
const spec = await serv.generateSpec(contract, {
  title: 'My API',
  version: '1.0.0',
  apiPrefix: '/api/v1',
  description: '接口说明',
})
// spec 为 OpenAPI 3.1 Document 对象
```

## HTTP 配置（`ServHttpConfigInput`）

| 字段        | 默认值                                     | 说明                                                                                                   |
| ----------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| `apiPrefix` | `'/api/v1'`                                | oRPC OpenAPI handler 挂载前缀                                                                          |
| `health`    | `{ path: '/health', readyPath: '/ready' }` | 健康与就绪检查端点                                                                                     |
| `openapi`   | `false`                                    | OpenAPI JSON endpoint，显式开启                                                                        |
| `docs`      | `false`                                    | Scalar 文档页，显式开启；启用后自动挂载 `/_hai/scalar.js` 本地脚本路由                                 |
| `rpc`       | `false`                                    | 内部 RPC endpoint，显式开启；`loopback` / `private-network` 基于真实连接 IP，网关场景用 `gateway-only` |
| `transport` | `undefined`                                | 顶层配置；启用后默认挂载 `{apiPrefix}/_hai/key-exchange`                                               |

```ts
import type { ServHttpConfigInput } from '@h-ai/serv'

const http: ServHttpConfigInput = {
  apiPrefix: '/api/v1',
  openapi: { path: '/openapi.json' },
  docs: { path: '/docs' },
  health: { path: '/health', readyPath: '/ready' },
  rpc: false,
}
```

## Feature Procedures（默认实现）

`serv.implement(...).context().route(...).build()` 同步返回普通 router，可直接按嵌套属性选取 `@h-ai/serv/features/iam` 等默认 procedures。挂载 contract 只包含实际选用的路由；不要为复用当前用户或刷新功能而同时开放注册、OTP 或通用管理入口。

`@h-ai/serv` 内置了三个 feature 模块，开箱即用：

| 导入路径                      | 工厂函数                                          | 所需依赖                       |
| ----------------------------- | ------------------------------------------------- | ------------------------------ |
| `@h-ai/serv/features/iam`     | `createIamProcedures({ iam })`                    | `@h-ai/iam`                    |
| `@h-ai/serv/features/storage` | `createStorageProcedures({ storage, authorize })` | `@h-ai/storage` + 资源授权策略 |
| `@h-ai/serv/features/ai`      | `createAiProcedures({ ai })`                      | `@h-ai/ai`                     |

`authorize` 从可信 `context.session` 校验资源所有者/租户，不能只信任客户端前缀；缺失策略时默认拒绝。列表会把 prefix 交给策略，空 prefix 不应获准全桶列举；批量全部授权后才执行。示例用户根为 `users/<编码后的 userId>/`，管理员跨用户访问必须显式授权。

AI 默认 feature 把 objectId 绑定到可信 `context.session.userId`，客户端可省略；所有读写与记忆查询都使用授权后的主体。sessionId 与 objectId 组成复合会话，写入默认 sessionId 为 `default`。其它 objectId 默认被拒绝；跨用户管理需显式提供 `authorizeObject({ context, objectId, sessionId, operation })`，按可信权限及租户关系严格返回 true。

`chats.createCompletion` 只提供非流式 JSON 输出，stream 字段无论 true/false 都在 procedure 前以 HTTP 400 拒绝。完整 function tools（嵌套 JSON Schema/strict）、tool_choice（auto/none/required/已定义函数名）和角色消息直接传给模型，不自动执行工具。工具结果必填 tool_call_id，不能把未知字段静默裁剪为看似成功。流式调用使用应用自己的 endpoint 与 ai.llm.chatStream 输出契约。

```typescript
const aiProcedures = createAiProcedures({
  ai,
  authorizeObject: ({ context }) => context.session.permissions.includes('ai.manage'),
})
```

## 常见模式

### 自定义领域 procedure

```typescript
import type { ServContext } from '@h-ai/serv'
import { serv } from '@h-ai/serv'
import { myContract } from './my-contract.js'

export function createMyProcedures() {
  return serv
    .implement(myContract)
    .context<ServContext>()
    .route('widget.list')
    .auth()
    .handle(({ input }) => widgetService.list(input))
    .route('widget.create')
    .permission('widget:write')
    .handle(({ input }) => widgetService.create(input))
    .build()
}
```

### 自定义 contract 组合

```typescript
import { apiContract } from '@h-ai/api-contract'
import { widgetContract } from './widget-contract.js'

export const myAppContract = apiContract.create({
  iam: apiContract.iam,
  widget: widgetContract,
})

const app = serv.createApp({
  contract: myAppContract,
  procedures: {
    iam: createIamProcedures({ iam }),
    widget: createMyProcedures(),
  },
  http: { apiPrefix: '/api/v1' },
})
```

## 错误码

route 实现器内置以下错误：

| 错误码                          | 触发条件                                                        |
| ------------------------------- | --------------------------------------------------------------- |
| `HaiCommonError.UNAUTHORIZED`   | `.auth()`：`context.session` 为空（无 token 或 token 校验失败） |
| `HaiCommonError.FORBIDDEN`      | `.permission()` / `.role()`：无对应权限或角色                   |
| `HaiCommonError.INTERNAL_ERROR` | route handler 抛出未处理异常                                    |

## httpOnly Cookie 认证

将 refresh token 存储在 `HttpOnly` cookie 中，避免 XSS 风险（浏览器 JS 无法读取）。

先区分两个 token：

- `accessToken`：短期凭证，客户端放在内存，通过 `Authorization: Bearer <token>` 发送；
  `serv.parseRequestContext()` / `extractBearerToken()` 解析、`buildAuthContextFactory()` 校验的就是它。
- `refreshToken`：长期凭证，启用 `refreshCookie` 后只保存在 httpOnly cookie；
  它不会走 `Authorization`，也不会被 `extractBearerToken()` 读取，只会在 `/auth/refresh` 被服务端取出。
  受信原生客户端可通过 `nativeTokenTransport` 改用 JSON body，但必须同时校验客户端标识与 Origin。

### 适用场景

- 浏览器端（Web / H5）使用 `@h-ai/api-client` 的 `apiClient.tokenStorage.httpOnlyCookie()`
- 对 refresh token 有更高安全要求，不希望存储在 localStorage

### 工作流程

1. 浏览器 POST `/auth/login` → serv 拦截 oRPC 成功响应 → `Set-Cookie: hai_refresh_token=...;HttpOnly;SameSite=Strict`，并从 JSON 响应体擦除 `refreshToken`
2. Access token 由客户端存内存（不持久化）
3. Access token 过期 → 客户端 POST `/auth/refresh`（浏览器自动携带 cookie）
4. serv 读取 cookie → 调用 `iam.session.refresh` → 返回新 access token + 更新 cookie（响应体不暴露 refresh token）
5. POST `/auth/logout` → serv 清除 cookie（`Max-Age=0`）

原生客户端匹配 `nativeTokenTransport.isRequest` 后，登录/注册/刷新响应保留轮换后的
`refreshToken`，刷新请求从 `{ refreshToken }` body 读取，且不读写浏览器 Cookie。

若同时启用 `transport: { crypto, serverKeyRing, keyStore }`，`/auth/refresh` 也必须走加密链路；cookie-only 刷新请求允许空 body，服务端不要把 `Request.body === null` 误判为需要解密的请求体。

### 配置

```typescript
const app = serv.createApp({
  contract,
  procedures,
  http: { apiPrefix: '/api/v1' },
  iam, // 顶层句柄：同时驱动 verifyToken 与 refresh
  refreshCookie: {
    // cookieName?: string              默认 'hai_refresh_token'
    // maxAge?: number                  默认 30 * 24 * 3600（30 天）
    // secure?: boolean                 默认 NODE_ENV 或 HAI_CORE_ENV 为 production 时开启；测试开关不放宽
    // onRefresh?: (token) => ...       可选：覆盖默认的 iam.session.refresh
    nativeTokenTransport: {
      // 必须 fail-closed：同时校验平台标识与允许的原生 Origin。
      isRequest: request =>
        request.headers.get('x-client-platform') === 'capacitor'
        && security.isNativeOrigin(request.headers.get('origin') ?? ''),
    },
  },
  // ✅ 顶层 iam 同时为 access token 校验提供默认实现，无需再传 verifyToken
})
```

> **逃脱口**：不使用 `@h-ai/iam` 时，可显式传入顶层 `verifyToken: token => myAuthService.verify(token)`。

### Cookie 规格

| 属性     | 值                                                   |
| -------- | ---------------------------------------------------- |
| Name     | `hai_refresh_token`（可配置）                        |
| Path     | `{apiPrefix}/auth/refresh`（最小范围）               |
| HttpOnly | ✓                                                    |
| SameSite | `Strict`                                             |
| Secure   | 生产自动开启，`secure: false` 可在 HTTP 开发环境关闭 |
| Max-Age  | 30 天（可配置）                                      |

## 测试

```bash
pnpm --filter @h-ai/serv test
pnpm --filter @h-ai/serv typecheck
pnpm --filter @h-ai/serv lint
```

标准登出入口 `auth.logout` 从 input.accessToken 或 Authorization Bearer 取得要撤销的完整令牌，不要求 access 仍有效。它不接受 userId 授权或撤销目标；缺少令牌返回失败，未知令牌幂等成功。共享存储故障原样保留失败 HaiResult，Cookie middleware 只对成功结果清理 refresh Cookie。普通 procedure 继续使用既有 HTTP 200 / HaiResult 协议，客户端必须检查 success，不能只检查状态码。

声明 JSDoc 的公共例子只从实际根入口或 features 子路径导入，内部 Hono/guard/router/transport 例子标注“模块内部上下文”。应用使用所安装版本的公开类型，并通过本应用已有的类型检查验证使用示例。启动工厂可显式抛配置异常，领域 procedure 必须检查 HaiResult；HTTP middleware 与普通 procedure 的状态语义分别处理。监听句柄 close 返回 Promise<void>，并不提供 serv.init/close 或替宿主管理业务模块。默认 AI JSON procedure 不支持 stream。

toFetch 统一使用标准 Request(input, init)：Request 输入也应用 method/headers/body/signal 等覆盖；非法、已消费或锁定正文以 Promise 拒绝。取消在派发前阻止 handler，进行中终止响应等待并传递有效 request.signal；完成/失败/取消均移除 adapter 监听器，丢弃的迟到响应会取消其正文。取消不回滚外部效果，Response 交付后的流由应用自行监听 request.signal 清理。原 Request 可否继续使用其正文遵循标准 Request 的流所有权规则，不承诺重复消费正文。
