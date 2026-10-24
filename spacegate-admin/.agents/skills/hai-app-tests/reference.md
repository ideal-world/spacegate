# hai-app-tests — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

新增/改变业务行为或修复缺陷时先用目标用例定义行为，再实现。按 [development.md](../hai-app-create/development.md) 的需求与安全边界选择测试层；纯文案、格式、文档或无行为变化的可逆低影响修改按影响验证，不补只匹配文本/结构的镜像测试。

## 适用场景

- 以 TDD 方式开发新功能（先写测试 → 确认失败 → 再实现 → 确认通过）
- 为当前架构的公共服务入口编写单元/集成测试
- 为 API 端点编写 E2E 测试（Playwright）
- 为页面交互编写 UI E2E 测试（Playwright）
- 为工具函数、Schema 校验编写单元测试
- 补齐有具体行为风险的验证缺口；覆盖率是定位线索，不是验收目标

## 行为验证与 TDD 工作流

不要求每个改动同时新增单元与 E2E。先复用现有 fixture 与目标用例，按变化选最小适用层；跨 contract/服务/客户端或 UI 的关键流程仍需相应集成/端到端验证。

### 阶段一：需求分析（Red 前准备）

1. **理解需求**：明确功能的输入、输出、边界条件、错误场景
2. **拆分测试点**：将需求转化为可验证的测试用例清单
3. **确定测试类型**：
   - **单元测试（Vitest）**：服务层、工具函数、Schema 校验、HaiResult 处理
   - **集成测试**：contract/procedure、事务、持久化、生命周期/失败清理、模块配置的真实组合
   - **E2E 测试（Playwright）**：API 边界、页面交互、认证流程与跨层用户路径

### 阶段二：Red — 编写失败的测试

1. **编写目标用例**：在适用的单元/集成/E2E 层验证目标变化，不重复创建其它层仅为打勾
2. 对本次相关的正常、输入边界、越权/跨租户、依赖失败、事务/幂等/清理场景给出可观察断言；纯工具无需添加无关权限路径
3. **运行测试确认目标用例因预期行为缺失而失败（仍适用的既有回归保持通过）**；明确新需求替代的模板断言同步更新并记录依据，不能为迁就实现改预期：
   ```bash
   pnpm --filter <app-name> test        # 目标位于单元/集成时使用
   pnpm --filter <app-name> test:e2e    # 目标位于 E2E 时使用；脚本以实际 package.json 为准
   ```
4. 新增用例若已通过，先确认现有实现是否已满足需求；仅在用例没有覆盖目标行为时修正

### 阶段三：Green — 最小实现

1. 参照 `hai-app-create` 技能编写实现代码
2. 每实现一个功能点，立即运行对应测试确认通过
3. 只写让测试通过的最小代码，不做过度设计
4. 运行受影响测试及必要回归确认通过（按影响选择范围）：
   ```bash
   pnpm --filter <app-name> test        # 单元测试 — 应全部 PASS
   pnpm --filter <app-name> test:e2e    # E2E 测试 — 应全部 PASS
   ```

### 阶段四：Refactor — 重构优化

1. 参照 `hai-app-review` 技能审查代码规范
2. 重构后执行受影响测试与必要回归，确认已有行为保持通过
3. 多项门禁按 [分轮验证与集中修复](validation.md) 收集一轮结果，汇总后集中修复；先定向确认失败和直接受影响项均已解决，再执行较完整验收。定向回归未通过时继续小范围修复，不从头重复全量门禁。PowerShell 使用 `pnpm.cmd`

## 测试目录约定

以下是常见组织方式；沿当前应用/workspace 的测试目录、alias 与脚本，不为目录一致性移动已有测试。

```
tests/                             # 单元测试（Vitest）
  services/
    user.test.ts                   # 服务层测试
    article.test.ts
  schemas/
    user-schema.test.ts            # Schema 校验测试
  utils/
    helper.test.ts                 # 工具函数测试
e2e/                               # E2E 测试（Playwright）
  helpers.ts                       # E2E 共用工具函数
  health.spec.ts                   # 健康检查 API
  auth-api.spec.ts                 # 认证 API 测试
  auth-ui.spec.ts                  # 认证页面 UI 测试
  users-api.spec.ts                # 用户管理 API 测试
  users-ui.spec.ts                 # 用户管理页面 UI 测试
```

## 核心规则

1. **行为测试先行**：新增/改变业务行为和修复缺陷时先定义目标用例；纯文案/格式/文档按影响验证。
2. **统一入口**：通过框架模块公共 API 测试（`reldb.sql`、`iam.auth`），不直接调用内部实现。
3. **从实际场景出发**：测试用例验证真实业务行为，不做形式覆盖。
4. **先审查后修复**：测试不通过时先审查业务逻辑是否有问题，不直接修改测试迎合通过。
5. **明确隔离层**：单元测试沿真实注入边界 mock 外部依赖；DB/缓存集成用内存 SQLite 或隔离环境/Testcontainers，不能用 mock 证明真实持久化/事务。
6. **区分证据**：端到端运行完整应用；可用受控外部依赖验证确定性错误路径，但报告替身范围。浏览器通过不等于真实 provider 通过；真实数据库/付费 provider/原生平台各自保留实际执行结果，缺环境说明未验证。

## 覆盖范围

按本次改变的行为选择适用路径；下表是设计依据，不要求每个路径在单元与 E2E 重复一遍。纯函数不必虚构权限场景，受保护资源则不能省掉未登录/缺权限与跨用户/租户验收：

| 路径     | 说明                     | 单元测试示例       | E2E 测试示例            |
| -------- | ------------------------ | ------------------ | ----------------------- |
| 正常路径 | 核心功能可用             | 创建用户成功       | POST /api/users → 201   |
| 边界路径 | 非法输入、空值、格式错误 | 空名称、超长字符串 | 空表单提交 → 校验提示   |
| 权限路径 | 未认证、无权限           | 未登录调用服务     | 检查真实失败包络/HTTP 状态，kit 页面按实际策略重定向 |
| 错误路径 | 服务异常、数据库错误     | DB 失败时的 HaiResult | 服务不可用 → 错误提示页 |

## 单元测试（Vitest）

### SQLite 持久化集成示例

```typescript
import { reldb } from '@h-ai/reldb'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'

// 内存 SQLite 隔离数据；应用服务测试同样从服务的公共入口验证结果。
beforeAll(async () => {
  const init = await reldb.init({ type: 'sqlite', database: ':memory:' })
  expect(init.success).toBe(true)
  const table = await reldb.ddl.createTable('articles', {
    id: { type: 'TEXT', primaryKey: true },
    title: { type: 'TEXT', notNull: true },
  })
  expect(table.success).toBe(true)
})
afterAll(async () => {
  const closed = await reldb.close()
  expect(closed.success).toBe(true)
})

describe('article persistence', () => {
  it('creates and reads the stored article', async () => {
    const articles = reldb.crud.table<{ id: string, title: string }>({ table: 'articles', dbType: 'sqlite' })
    const saved = await articles.create({ id: 'a1', title: 'Test' })
    expect(saved.success).toBe(true)
    const found = await articles.findById('a1')
    expect(found.success).toBe(true)
    if (found.success) expect(found.data).toEqual({ id: 'a1', title: 'Test' })
  })
})
```

本例验证框架真实持久化，不代替应用服务验收。应用另测相关权限、校验和 DB 失败透传；mock 必须沿真实注入边界或 CRUD 仓库实例配置，create 返回 ExecuteResult，不能伪造为实体。

### Schema 校验测试模板

```typescript
import { CreateUserSchema } from '$lib/server/schemas/user'
// tests/schemas/user-schema.test.ts
import { describe, expect, it } from 'vitest'

describe('CreateUserSchema', () => {
  it('should accept valid input', () => {
    const result = CreateUserSchema.safeParse({
      username: 'testuser',
      email: 'test@example.com',
      password: '12345678',
    })
    expect(result.success).toBe(true)
  })

  it('should reject short username', () => {
    const result = CreateUserSchema.safeParse({
      username: 'ab',
      email: 'test@example.com',
      password: '12345678',
    })
    expect(result.success).toBe(false)
  })

  it('should reject invalid email', () => {
    const result = CreateUserSchema.safeParse({
      username: 'testuser',
      email: 'not-email',
      password: '12345678',
    })
    expect(result.success).toBe(false)
  })
})
```

## E2E 测试（Playwright）

### Playwright 配置

优先复用 CLI 已生成的 `playwright.config.ts`。下列为 SvelteKit preview 示例；API/fullstack/Vite 使用自身已有 webServer/服务组合与 scripts，不照搬 `pnpm preview`：

```typescript
// playwright.config.ts
import process from 'node:process'
import { defineConfig } from '@playwright/test'

const DEFAULT_BASE_URL = 'http://localhost:4173'
const baseURL = process.env.BASE_URL || DEFAULT_BASE_URL
const previewHost = resolveHostFromBaseUrl(baseURL, 'localhost')
const previewPort = resolvePortFromBaseUrl(baseURL, '4173')

function resolveHostFromBaseUrl(urlString: string, fallbackHost: string): string {
  try {
    const url = new URL(urlString)
    return url.hostname || fallbackHost
  }
  catch {
    return fallbackHost
  }
}

function resolvePortFromBaseUrl(urlString: string, fallbackPort: string): string {
  try {
    const url = new URL(urlString)
    if (url.port)
      return url.port
    return url.protocol === 'https:' ? '443' : '80'
  }
  catch {
    return fallbackPort
  }
}

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 2,
  reporter: 'list',
  timeout: 30_000,
  use: {
    baseURL,
    channel: 'chrome',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: `pnpm build && pnpm preview --host ${previewHost} --port ${previewPort} --strictPort`,
    env: { HAI_E2E: '1', BASE_URL: baseURL },
    url: baseURL,
    reuseExistingServer: false,
    timeout: 180_000,
  },
})
```

### E2E 工具函数模板

以下登录路径、表单字段与 API 仅作 SvelteKit 示例，必须替换为当前应用路由/认证流程；API service/typed client 测试从现有 contract 读取路径，不维护第二份路由常量。

```typescript
// e2e/helpers.ts
import type { APIRequestContext, Page } from '@playwright/test'

/** 生成唯一测试用户 */
export function uniqueUser(prefix = 'e2e') {
  const safePrefix = (prefix.replace(/\W/g, '') || 'e2e').slice(0, 8)
  const entropy = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`
  const id = entropy.slice(-10)
  const username = `${safePrefix}_${id}`.slice(0, 20)
  return {
    username,
    email: `${safePrefix}_${id}@test.local`,
    password: 'Test1234!@',
  }
}

/** 通过 API 注册用户 */
export async function registerViaApi(
  request: APIRequestContext,
  user: ReturnType<typeof uniqueUser>,
) {
  return request.post('/api/auth/register', {
    data: {
      username: user.username,
      email: user.email,
      password: user.password,
      confirmPassword: user.password,
    },
  })
}

/** 注册并登录，返回已认证的 Page */
export async function registerAndLogin(
  page: Page,
  request: APIRequestContext,
  prefix = 'e2e',
) {
  const user = uniqueUser(prefix)
  await registerViaApi(request, user)
  await page.goto('/login')
  await page.fill('[name="username"]', user.username)
  await page.fill('[name="password"]', user.password)
  await page.click('button[type="submit"]')
  await page.waitForURL('/')
  return user
}
```

### API 端点 E2E 测试模板

下例是返回 kit Response 的 SvelteKit 端点。serv/oRPC 普通 procedure 可能以 HTTP 200 返回失败 HaiResult，需同时检查 `success/error.code`；不能把 `error.httpStatus` 当作 response.status，也不能统一套用下列 401/201 断言。输入校验与传输错误另按实际状态测试。

```typescript
// e2e/articles-api.spec.ts
import { expect, test } from '@playwright/test'
import { registerAndLogin } from './helpers'

test.describe('Articles API', () => {
  test('GET /api/articles 未认证返回 401', async ({ request }) => {
    const response = await request.get('/api/articles')
    expect(response.status()).toBe(401)
  })

  test('POST /api/articles 创建文章成功', async ({ page, request }) => {
    await registerAndLogin(page, request, 'article')

    const response = await page.request.post('/api/articles', {
      data: { title: 'E2E Test Article', content: 'Test content' },
    })
    expect(response.status()).toBe(201)

    const body = await response.json()
    expect(body.success).toBe(true)
    expect(body.data.title).toBe('E2E Test Article')
  })

  test('POST /api/articles 空标题返回 400', async ({ page, request }) => {
    await registerAndLogin(page, request, 'article')

    const response = await page.request.post('/api/articles', {
      data: { title: '', content: '' },
    })
    expect(response.status()).toBe(400)
  })

  test('GET /api/articles 认证后获取列表', async ({ page, request }) => {
    await registerAndLogin(page, request, 'article')

    const response = await page.request.get('/api/articles')
    expect(response.ok()).toBeTruthy()

    const body = await response.json()
    expect(body.success).toBe(true)
    expect(Array.isArray(body.data)).toBe(true)
  })
})
```

### 页面 UI E2E 测试模板

```typescript
// e2e/articles-ui.spec.ts
import { expect, test } from '@playwright/test'
import { registerAndLogin } from './helpers'

test.describe('Articles Page', () => {
  test('未登录跳转登录页', async ({ page }) => {
    await page.goto('/admin/articles')
    await expect(page).toHaveURL(/login/)
  })

  test('已登录显示文章列表', async ({ page, request }) => {
    await registerAndLogin(page, request, 'artui')
    await page.goto('/admin/articles')

    await expect(page.locator('h1, h2').first()).toContainText(/文章|article/i)
  })

  test('创建文章表单提交', async ({ page, request }) => {
    await registerAndLogin(page, request, 'artui')
    await page.goto('/admin/articles/new')

    await page.fill('[name="title"]', 'E2E Article')
    await page.fill('[name="content"]', 'E2E content')
    await page.click('button[type="submit"]')

    // 验证跳转或成功提示
    await expect(page.locator('.alert-success, .toast')).toBeVisible({ timeout: 5000 })
  })
})
```

## TDD 用例设计策略

以下展示候选验证点，实际按职责和已有用例选择；不要求将同一输入规则在所有层重复实现和测试。

### 从需求到测试用例的转换

```
需求：用户可以创建文章，标题必填且不超过 100 字符，需要登录后才能操作

↓ 拆分测试点

单元测试（Vitest）：
  ✅ 创建文章成功返回 ok(data)
  ✅ 空标题被当前边界 Schema 拒绝
  ✅ 超长标题（>100 字符）被拒绝
  ✅ DB 异常时透传错误

Schema 测试（Vitest）：
  ✅ 合法输入通过校验
  ✅ 空标题被拒绝
  ✅ 超长标题被拒绝
  ✅ 缺少必填字段被拒绝

E2E 测试（Playwright）：
  ✅ POST /api/articles 未认证 → 401
  ✅ POST /api/articles 合法数据 → 201
  ✅ POST /api/articles 空标题 → 400
  ✅ 页面表单提交成功 → 跳转/提示
  ✅ 未登录访问创建页 → 重定向登录
```

## 断言规范

- HaiResult 型 API 校验 `result.success`；其它返回、显式抛错与流式 API 按实际签名验证
- 失败时校验 `error.code`（不只检查 `success === false`）
- 使用类型缩窄后访问 data 或 error
- E2E 断言使用 Playwright 内置 expect 匹配器；同时检查用户可见结果和必要持久化状态，不能只断言 HTTP 200、按钮点击或成功提示
- 失败验收检查错误码/状态与“没有副作用”；事务中途失败、重复回调/写入、取消/关闭等按需求检查一致性与资源退出

```typescript
// ✅ 单元测试断言
const result = await service.create(input)
expect(result.success).toBe(true)
if (result.success) {
  expect(result.data.id).toBeDefined()
}

// ✅ 单元测试错误断言
const result = await service.create(invalidInput)
expect(result.success).toBe(false)
if (!result.success) {
  expect(result.error.code).toBe('VALIDATION_FAILED')
}

// ✅ E2E API 断言
const response = await request.post('/api/articles', { data: input })
expect(response.status()).toBe(201)
const body = await response.json()
expect(body.success).toBe(true)

// ✅ E2E UI 断言
await expect(page.locator('.alert-success')).toBeVisible()
await expect(page).toHaveURL('/admin/articles')
```

## 质量门禁

从当前 package.json 确认实际脚本和 workspace 名称，按 [分轮验证与集中修复](validation.md) 组织以下适用检查；不要为格式/文档改动运行无关 provider 或新增镜像测试。逐轮记录范围、状态/退出码、日志、mock/真实环境与未验证原因，完整日志按需读取。

```bash
# 运行单元测试
pnpm --filter <app-name> test

# 运行 E2E 测试
pnpm --filter <app-name> test:e2e

# 覆盖率
pnpm --filter <app-name> test:coverage

# 全量门禁
pnpm typecheck
pnpm lint
pnpm build # 构建/配置/跨包变更
pnpm test
```

## 相关 Skills

- `hai-app-create`：应用功能创建规范（TDD Green 阶段参照）
- `hai-app-review`：应用代码审查规范（TDD Refactor 阶段参照）
- `hai-build`：项目架构总览与 TDD 工作流导航
