# hai-build — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

应用 Node/工具链要求以当前 `package.json.engines`、lockfile 与 CI 为准。使用 Turbo 时核对影响产物的环境变量、共享配置和模式切换清理策略，并验证实际产物；不要假定应用使用框架仓库的 tsup/Turbo 私有脚本。

本技能帮助识别应用架构、依赖和技能。日常开发决策、安全边界与能力选择见 [development.md](../hai-app-create/development.md)。

## 适用场景

- 初次接触项目，需要了解整体架构与模块关系
- 不确定应使用哪个技能来完成任务
- 需要了解模块初始化顺序与依赖关系
- 需要执行跨模块操作或全局配置变更
- 多端构建切换（SSR ↔ SPA ↔ 原生 App）

## 项目架构

### 先辨别当前项目

| 架构 | 接口/服务 | UI 与客户端 | 常见位置（以当前项目为准） |
| --- | --- | --- | --- |
| SvelteKit | `@h-ai/kit`、文件路由与 server services | Svelte 组件与既有数据加载 | `src/routes/`、`src/lib/server/` |
| API workspace | `@h-ai/api-contract` + `@h-ai/serv` | 无需页面 | `apps/*-contract`、`apps/*-service` |
| fullstack | contract + serv，共享业务结构 | Vite Svelte Web/所选原生前端，typed client | `packages/*-contract`、`packages/*-serv`、`packages/*-shared`、`apps/*` |
| Vite SPA/原生 | 远端服务 contract | `@h-ai/api-client` 与适用平台适配 | `src/App.svelte`、`src/lib/api.ts` 或 shared 客户端 |

不要从目录名称推断运行环境；沿入口、导入与 scripts 核实。下列 SvelteKit 示例只适用于对应项目。

### 技术栈

| 层次     | 技术                           |
| -------- | ------------------------------ |
| UI     | Svelte 5 (Runes)；SvelteKit 或 Vite 以项目为准 |
| 样式     | TailwindCSS 4 + DaisyUI 5      |
| 语言     | TypeScript 6 / 7 (strict)       |
| 构建     | Vite + tsup                    |
| 包管理   | pnpm 12                        |
| 单元测试 | Vitest                         |
| E2E 测试 | Playwright                     |
| 原生 App | Capacitor / Tauri / 小程序适配按当前项目选择 |
| 公共 API | oRPC contract + Hono + typed client |
| 认证     | Bearer access token + httpOnly refresh cookie（浏览器推荐） |

pnpm 专属配置放在 `pnpm-workspace.yaml`，`.npmrc` 仅用于注册表与认证。Docker 缓存层使用 `pnpm fetch` 读取锁文件；pnpm 12 的该命令不支持 `--frozen-lockfile`。后续仍通过 `pnpm install --offline --frozen-lockfile` 校验清单与锁文件一致。容器裁剪用 `pnpm pm deploy --filter <服务包> --prod <目标目录>`，确保执行内置命令，避免项目同名 `deploy` 脚本覆盖它。保留 `.gitattributes`，让 `.dockerignore`、Dockerfile 和 POSIX 脚本统一使用 LF，保持跨平台输入一致并确保 Linux 脚本可运行。

### SvelteKit Adapter 模式

仅在项目实际使用 SvelteKit 和 `@h-ai/kit/adapter` 时使用；Vite SPA 不配置 SvelteKit adapter，fullstack 各前端按自身 scripts 构建。

| 模式    | Adapter        | 认证方式              | 部署目标                |
| ------- | -------------- | --------------------- | ----------------------- |
| SSR Web | adapter-node   | 项目现有 Cookie/会话管道 | Node.js 服务器          |
| SPA     | adapter-static | Bearer access token + httpOnly refresh cookie | CDN / 静态托管          |
| Android | adapter-static | Bearer access token + Capacitor secure storage | Capacitor → Android APK |
| iOS     | adapter-static | Bearer access token + Capacitor secure storage | Capacitor → iOS IPA     |

**Adapter 切换**：

```typescript
// svelte.config.js — 使用 kit 提供的 createAdapter
import { createAdapter } from '@h-ai/kit/adapter'

const config = {
  kit: {
    adapter: createAdapter(), // 根据 VITE_ADAPTER 环境变量自动选择
  },
}
```

- `VITE_ADAPTER=node`（默认）→ adapter-node（SSR）
- `VITE_ADAPTER=static` → adapter-static（SPA / 原生 App）

### SvelteKit 目录示例

```
项目根/
  config/                         # 模块配置（YAML）
    _core.yml                     # Core 配置（必须）
    _db.yml                       # DB 配置（按需）
    _cache.yml                    # Cache 配置（按需）
    _iam.yml                      # IAM 配置（按需）
    _storage.yml                  # Storage 配置（按需）
    _ai.yml                       # AI 配置（按需）
  src/
    app.html                      # HTML 入口
    app.d.ts                      # 全局类型声明
    hooks.server.ts               # 服务端 Hook（模块初始化 + 请求管道）
    lib/
      api.ts                      # @h-ai/api-client 初始化（SPA / 原生 App 使用）
      capacitor.ts                # Capacitor 初始化（原生 App）
      server/
        init.ts                   # 模块初始化入口（单例，SSR）
      paraglide/                  # i18n 生成文件（禁止手动修改）
      components/                 # 应用组件
    routes/
      +layout.svelte              # 根布局
      +page.svelte                # 首页
      (auth)/                     # 认证相关页面组
      api/                        # API 端点
        v1/                       # API v1 版本
          payment/                # 支付端点（按需）
  static/                         # 静态资源
  messages/                       # i18n 翻译文件
  capacitor.config.ts             # Capacitor 配置（原生 App）
```

### 模块依赖图

```
core（基础能力：配置、日志、i18n、HaiResult）
  ├── crypto（加密：SM2/SM3/SM4）
  ├── reldb（数据库：SQLite/PostgreSQL/MySQL）
  ├── cache（缓存：内存/Redis）
  ├── storage（存储：本地/S3）
  ├── ai（AI：LLM/MCP/Agent）
  ├── iam（身份管理）← 依赖 crypto + reldb + cache
  ├── payment（支付）← 依赖 reldb + crypto
  ├── api-contract（公共 HTTP API 契约）← 纯定义，依赖 core + zod + oRPC contract
  ├── serv（Hono + oRPC API Service）← 依赖 api-contract；features 按需依赖 iam/storage/ai
  ├── kit（SvelteKit 集成）← Hook / guard / validate / response，不定义公共 contract
  ├── api-client（typed API 客户端）← 依赖 api-contract + oRPC client
  ├── capacitor（原生能力）← 纯浏览器端
  └── ui（UI 组件库）← 依赖 core
```

这是常见关联；具体配置/依赖以所选模块当前版本为准。其它能力（reach/audit/scheduler/datapipe/vecdb）及同模块 feature 的任务导航见 [需求能力表](../hai-app-create/development.md#将需求映射到现有能力)。

### 模块初始化顺序

#### SSR 模式（hooks.server.ts）

在 `src/lib/server/init.ts` 中按依赖顺序初始化：

```typescript
import { core } from '@h-ai/core'
import { cache, CacheConfigSchema } from '@h-ai/cache'
import { reldb, ReldbConfigSchema } from '@h-ai/reldb'
import { iam, IamConfigSchema } from '@h-ai/iam'

// 放入应用已有的启动函数；并发防护和关闭钩子复用当前模板。
core.init({ configDir: './config' })
const dbConfig = core.config.validate('db', ReldbConfigSchema)
if (!dbConfig.success) return dbConfig
const cacheConfig = core.config.validate('cache', CacheConfigSchema)
if (!cacheConfig.success) return cacheConfig
const iamConfig = core.config.validate('iam', IamConfigSchema)
if (!iamConfig.success) return iamConfig
const db = await reldb.init(dbConfig.data)
if (!db.success) return db
const cached = await cache.init(cacheConfig.data)
if (!cached.success) {
  const cleanup = await reldb.close()
  return cleanup.success ? cached : cleanup
}
const identity = await iam.init(iamConfig.data)
if (!identity.success) {
  const cacheCleanup = await cache.close()
  const dbCleanup = await reldb.close()
  if (!cacheCleanup.success) return cacheCleanup
  if (!dbCleanup.success) return dbCleanup
  return identity
}
// 应用结束时 await iam.close()、cache.close()、reldb.close()；
// 检查关闭结果；core 无 close，kit/api-contract/serv 不需要 init。
```

#### SPA / 原生 App 模式（已有 api.ts 或 shared 客户端）

复用当前项目初始化函数、API base 运行时配置和 TokenStorage；以下仅展示浏览器 Cookie 模式。原生端按 `hai-api-client` / `hai-capacitor` 配置安全存储，不用 localStorage 持久化敏感 token。

```typescript
import { apiClient } from '@h-ai/api-client'

export async function initApi() {
  return apiClient.init({
    baseUrl: configuredApiBase, // 来自当前项目已校验的运行时配置
    auth: {}, // 浏览器默认 httpOnly cookie；原生 App 传入 Capacitor TokenStorage
    timeout: 15_000,
  })
}

export { apiClient }
```

#### 原生 App 追加初始化（src/lib/capacitor.ts）

```typescript
import { capacitor } from '@h-ai/capacitor'

export async function initCapacitor() {
  const initialized = await capacitor.init()
  if (!initialized.success) return initialized
  return capacitor.statusBar.configure({ style: 'dark' })
}
```

### 配置文件格式

所有模块配置使用 YAML 保存默认值；`HAI_<配置名>_<YAML 路径>` 环境变量
自动覆盖对应叶子项，且优先级更高：

```yaml
# config/_core.yml
name: my-app
env: development
logging:
  level: info
```

以上字段分别对应 `HAI_CORE_NAME`、`HAI_CORE_ENV`、
`HAI_CORE_LOGGING_LEVEL`。

## 技能导航

根据任务类型选择正确的技能：

### 模块使用（API 与集成）

先按 [development.md 的需求能力表](../hai-app-create/development.md#将需求映射到现有能力) 匹配需求，再查以下 API 入口；不用通读所有技能。

| 任务                  | 技能             | 触发关键词                                        |
| --------------------- | ---------------- | ------------------------------------------------- |
| 配置/日志/i18n/HaiResult | `hai-core`       | core.init, core.logger, core.config, HaiResult       |
| 数据库操作            | `hai-reldb`      | reldb.init, SQL, CRUD, 事务, 分页, DDL               |
| 缓存操作              | `hai-cache`      | cache.init, cache.kv.get/set, TTL, Redis             |
| 文件存储              | `hai-storage`    | storage.init, 上传, 下载, S3, 本地存储            |
| 加密/签名/哈希        | `hai-crypto`     | crypto.init, SM2, SM3, SM4, 加密, 签名            |
| 身份认证/授权         | `hai-iam`        | iam.init, 登录, 注册, RBAC, Token, Bearer         |
| AI/LLM/MCP            | `hai-ai`         | ai.init, LLM, MCP, Agent, 工具调用                |
| 公共 API 契约         | `hai-api-contract` | apiContract.create, contract, schema, oRPC, HaiResult |
| API Service 运行时    | `hai-serv`       | serv.createApp, Hono, procedures, OpenAPI, docs, route guard |
| SvelteKit 集成        | `hai-kit`        | kit.createHandle, guard, middleware, validate, response |
| UI 组件               | `hai-ui`         | 表单, 按钮, 表格, Modal, Toast, 移动端组件        |
| typed API 客户端      | `hai-api-client` | apiClient.init, typed client, Bearer, 401 refresh, custom fetch |
| 原生 App 能力         | `hai-capacitor`  | capacitor, 相机, 推送, 状态栏, 设备信息           |
| 支付                  | `hai-payment`    | payment, 微信支付, 支付宝, Stripe, 订单           |

### 开发流程

| 任务                   | 技能             | 触发关键词                             |
| ---------------------- | ---------------- | -------------------------------------- |
| 新增/修改/修复应用功能 | `hai-app-create` | 页面, 组件, API, 模型, 功能修复 |
| 代码审查与规范化       | `hai-app-review` | 审查, review, 规范, 优化               |
| 编写/补充测试          | `hai-app-tests`  | 测试, test, 单测, 覆盖率, TDD, E2E     |
| CI/CD 与质量门禁       | `hai-ci`         | CI, CD, GitHub Actions, workflow, secret scan |
| PR / Issue 交付审查    | `hai-pr-review`  | PR, pull request, issue, CODEOWNERS, 自动 Review, AI Review |
| hai-framework 升级与同步 | `hai-framework-sync` | 升级框架, 最新版本, 版本差异, skills 同步, npm/local 漂移 |

## 行为开发工作流

新增/改变行为时：先写有意义的目标用例 → 确认因目标行为缺失失败 → 实现 → 通过 → 审查。纯文案、格式和文档变化按影响验证，不机械新增单元与 E2E 测试。

### 完整流程

```
需求分析 → 设计测试用例 → 编写测试（Red）→ 运行测试确认失败
    → 编写实现（Green）→ 运行测试确认通过
    → 重构优化（Refactor）→ 运行质量门禁
```

### 按阶段使用的技能

| 阶段                | 主要技能         | 说明                                       |
| ------------------- | ---------------- | ------------------------------------------ |
| 需求分析 + 测试设计 | `hai-app-tests`  | 拆分测试点，确定单元测试与 E2E 测试        |
| Red：编写测试       | `hai-app-tests`  | 编写 Vitest 单元测试 + Playwright E2E 测试 |
| Green：编写实现     | `hai-app-create` | 按照创建规范编写服务/路由/组件             |
| Refactor：重构审查  | `hai-app-review` | 代码规范审查、分层检查、日志/i18n 合规     |

### 快速指令

```bash
# Red：运行测试确认新增目标用例因预期行为缺失而失败（既有回归保持通过）
pnpm --filter <app-name> test
pnpm --filter <app-name> test:e2e

# Green：运行测试确认全部通过
pnpm --filter <app-name> test
pnpm --filter <app-name> test:e2e

# Refactor：按下方分轮验证流程收集适用门禁
pnpm typecheck
pnpm lint
pnpm build # 构建/配置/模板/跨包契约变更
pnpm test
```

以上命令按 [分轮验证与集中修复](../hai-app-tests/validation.md) 调度，不在普通失败时立即修改并从头重跑。

## 统一编码规范

### 强制规则

- **禁止 `any`**：不确定类型用 `unknown`，在边界处做类型缩窄
- **禁止 `console.log`**：使用 `core.logger`（trace/debug/info/warn/error/fatal）
- **禁止硬编码密钥**：使用服务端配置/环境变量；新增变量在 `.env.example` 中写入占位符，PUBLIC/VITE 变量不得包含密钥
- **禁止无说明的 `as unknown as T`**：必要第三方断言写明依据；代码注释中文、日志英文并脱敏
- **禁止硬编码用户文案**：所有用户可见文本必须使用 i18n key
- **禁止修改 `src/lib/paraglide`** 生成文件

### HaiResult 模式

返回 `HaiResult<T>` 的领域操作按以下模式处理；工具、工厂和流式 API 以签名为准：

```typescript
const result = await reldb.sql.query('SELECT * FROM users')
if (!result.success) {
  // result.error: { code, message, details? }
  core.logger.error('Query failed', { error: result.error })
  return kit.response.internalError()
}
// result.data 可安全使用
return kit.response.ok(result.data)
```

### 质量门禁

按当前项目已定义的 scripts 和变更影响选择检查，执行 [分轮验证与集中修复](../hai-app-tests/validation.md)：普通失败先记录，独立检查继续，汇总后集中修复。PowerShell 使用 `pnpm.cmd`：

```bash
pnpm typecheck    # TypeScript 类型检查
pnpm lint         # ESLint 检查
pnpm build        # 构建/配置/模板/跨包契约变更
pnpm test         # Vitest 单元测试
```

## 标准工作流

### 变更前：需求分析 + 测试设计

1. 按 development.md 匹配适用框架能力、已有 feature/配置和实际版本，说明必要自定义缺口
2. 列出将修改的文件路径
3. 确认哪些类型/接口会变
4. 搜索确认引用关系（避免遗漏）
5. 按行为风险设计测试与安全验收场景

### 变更中：TDD 三步走

1. **Red**：为新增/改变行为编写适用的单元/集成/E2E 测试，确认目标用例失败、既有回归通过
2. **Green**：按技能指引完成功能实现，逐步让测试通过
3. **Refactor**：审查代码规范、重构优化

### 变更后：验证

1. 按影响范围调度一轮 typecheck/lint/build/test；UI/路由/端到端改动加相应 E2E，从 package.json 确认入口
2. 记录全部可执行结果与日志；真实前置失败只阻塞依赖项，普通失败不边测边改
3. 归并问题并集中修复，再验证失败、阻塞及受影响项；最终必需门禁对最终代码有效通过
4. 搜索确认所有引用点已更新，报告未验证边界与结果来源

涉及原生交付时按当前项目 Tauri/Gradle/Xcode 等实际入口打包，在目标 OS/设备验证安装启动、会话、安全 TokenStorage、API 域名及覆盖升级。Web 构建/E2E 不代替原生验收；记录平台/工具链与结果，缺平台写未验证。
