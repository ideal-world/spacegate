# hai-app-create — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

新增、修改和修复功能前先读 [development.md](development.md)，确认适用框架能力、当前安装版本与安全边界。这里的路由/页面示例适用于 SvelteKit；API workspace 围绕已有 contract/service 扩展，fullstack 使用现有 contract/serv/shared 和所选前端，不套用 SvelteKit 目录。

## 适用场景

- 新增页面（路由）
- 新增 API 端点
- 新增服务层模块（`$lib/server/services/`）
- 新增数据库表 / 数据模型
- 新增业务组件
- 修改或修复已有功能、接口和交互

## 行为开发与验证

新增/改变行为时采用下列流程；纯文案、格式或文档变化按影响验证，不为流程补无意义测试。

### Step 1：需求分析与测试设计

1. 明确功能的输入、输出、边界条件、错误场景
2. 按 development.md 匹配现有应用实现、框架模块及其 feature/配置；查公开类型与安装版本，再确定修改文件（服务、路由、Schema、组件等）
3. 设计正常、错误和适用安全边界的可观察验收（参照 `hai-app-tests`）；已有行为优先扩展现有用例

### Step 2：编写测试（Red 阶段）

1. **编写目标用例**：按当前项目测试结构为改变的服务行为、Schema、接口或交互选择单元/集成/E2E 测试，不要求每次创建两套测试
2. **保留既有回归**：新增目标用例不得依赖破坏现有实现才能失败
3. 新文件可先提供最小签名，但不能为制造 Red 把现有功能替换成空实现
4. **运行测试确认新增目标用例因预期行为缺失而失败（既有回归保持通过）**：
   ```bash
   pnpm --filter <app-name> test        # 新增目标用例应 FAIL，既有回归应 PASS
   ```

### Step 3：编写实现（Green 阶段）

按照下文的「创建指南」编写实际功能代码，每完成一个功能点立即运行测试确认通过。

### Step 4：重构与验证（Refactor 阶段）

1. 参照 `hai-app-review` 审查代码规范
2. 按 [分轮验证与集中修复](../hai-app-tests/validation.md) 收集适用门禁，普通失败不中断独立检查，汇总后集中修复、再验证；优先 workspace filter，PowerShell 使用 `pnpm.cmd`
3. UI/路由/端到端改动执行相应 E2E；报告失败路径与安全验收结果，未运行的边界说明原因

## 目录结构约定

以下为 SvelteKit 示例；其它架构沿当前项目目录，不生成这些文件。

```
src/
  hooks.server.ts                # `handle` hook 入口
  app.css                        # 全局样式（TailwindCSS 4）
  lib/
    server/
      init.ts                    # 应用初始化（模块 init 顺序）
      services/                  # 业务服务层（Server-only）
        index.ts                 # 服务聚合导出
        user.ts                  # 示例：用户服务
      schemas/                   # 请求校验 Schema（Zod）
    components/                  # 业务组件（非 @h-ai/ui 通用组件）
    stores/                      # 客户端 Store（Svelte 5 Runes）
    paraglide/                   # i18n 生成文件（禁止手动修改）
  routes/
    +layout.svelte               # 根布局
    +page.svelte                 # 首页
    (auth)/                      # 认证分组路由
      login/+page.svelte
    admin/                       # 管理区域
      users/
        +page.svelte             # 列表页
        +page.server.ts          # 列表数据加载
        [id]/
          +page.svelte           # 详情页
          +page.server.ts
    api/                         # API 端点
      auth/+server.ts
      health/+server.ts
      iam/
        users/+server.ts
  messages/                      # i18n 消息文件
    en-US.json
    ja-JP.json
    zh-CN.json
  config/                        # 模块配置文件
    _core.yml
    _db.yml
    _cache.yml
    _iam.yml
```

## 创建页面

### 页面路由规则

使用 SvelteKit 文件路由，参考 `src/routes/` 约定：

| 类型     | 路径示例                           | 说明                 |
| -------- | ---------------------------------- | -------------------- |
| 静态页面 | `routes/about/+page.svelte`        | 常规页面             |
| 动态路由 | `routes/users/[id]/+page.svelte`   | URL 参数             |
| 分组路由 | `routes/(auth)/login/+page.svelte` | 共享布局，不影响 URL |
| API 端点 | `routes/api/users/+server.ts`      | RESTful API          |

### 页面文件模板

```svelte
<!-- +page.svelte -->
<script lang="ts">
  // Svelte 5 Runes 语法
  let { data } = $props()
  let loading = $state(false)

  async function handleAction() {
    loading = true
    try {
      // 业务逻辑
    } finally {
      loading = false
    }
  }
</script>

<div class="container mx-auto p-4">
  <!-- 使用 @h-ai/ui 组件 -->
</div>
```

### 数据加载（+page.server.ts）

```typescript
import type { PageServerLoad } from './$types'
import { error, redirect } from '@sveltejs/kit'

// someService 与 safeLoadErrorMessage 由应用已有服务与请求 locale 的 i18n 提供。
export const load: PageServerLoad = async ({ locals }) => {
  if (!locals.session)
    redirect(303, '/auth/login')
  const result = await someService.list()
  if (!result.success)
    error(503, safeLoadErrorMessage)
  return { items: result.data }
}
```

## 创建 API 端点

API workspace/fullstack 先检查 [hai-api-contract](../hai-api-contract/SKILL.md) 的内置领域 contract 与 [hai-serv](../hai-serv/SKILL.md) 的 feature procedures；应用私有接口通过现有 contract → procedure → typed client 扩展。身份/权限守卫用 serv route 的 auth/permission/role，业务资源归属在服务层检查；不添加 `+server.ts` 或 kit.validate。

### SvelteKit RESTful 端点模板

本例依赖应用提供 `service`；`toHttpError(result.error)` 是应用已有或需实现的错误边界：保留安全错误码、映射 HTTP 状态，对内部失败使用当前 locale 的安全文案且不暴露 SQL/路径/凭据。不能不分类便直接透传内部错误消息。

```typescript
// src/routes/api/[resource]/+server.ts
import { kit } from '@h-ai/kit'
import { z } from 'zod'

const CreateSchema = z.object({
  name: z.string().min(1).max(100),
  description: z.string().optional(),
})

// GET - 列表
export const GET = kit.handler(async ({ locals }) => {
  kit.guard.require(locals.session, 'resource:read')

  const result = await service.list()
  if (!result.success)
    return toHttpError(result.error)
  return kit.response.ok(result.data)
})

// POST - 创建
export const POST = kit.handler(async ({ request, locals }) => {
  kit.guard.require(locals.session, 'resource:create')

  const data = await kit.validate.body(request, CreateSchema)

  const result = await service.create(data)
  if (!result.success)
    return toHttpError(result.error)
  return kit.response.created(result.data)
})
```

### 要点

- 使用 kit.guard.require / kit.validate.body 的 SvelteKit 端点用 `kit.handler(async ({ locals, request, ... }) => { ... })` 包裹，由 handler 统一处理 throw 的 Response；其它架构用对应运行时
- 权限守卫使用 `kit.guard.require(locals.session, 'xxx:yyy')`，它本身就是 throw 模式（未登录 throw 401、无权限 throw 403 Response），**无需**检查返回值
- 输入校验使用 `await kit.validate.body(request, Schema)`，校验失败时 throw 400 Response，**无需**手动判断 valid
- 返回统一使用 `kit.response.*`
- 服务层保留上游 HaiResult 错误；本例 SvelteKit HTTP 边界按安全消息和状态转换，不能把数据库失败作为 400 或空列表成功。serv/oRPC 普通 procedure 按真实 HaiResult 包络契约验收，不照搬本例状态映射
- HaiResult 型业务操作先检查 result.success；流和显式抛错 API 按各自契约处理

## 创建服务层

服务层位于 `$lib/server/services/`，处理业务逻辑。

```typescript
import type { HaiResult, PaginatedResult } from '@h-ai/core'
import { core, ok } from '@h-ai/core'
import { reldb } from '@h-ai/reldb'

// Article/CreateArticleInput/ListParams 为应用类型；已完成建表。
const articles = reldb.crud.table<Article>({
  table: 'articles',
  idColumn: 'id',
  dbType: 'sqlite', // 与本例初始化后端一致；切换数据库时同步修改
  select: ['id', 'title', 'created_at AS createdAt'],
})

export async function createArticle(input: CreateArticleInput): Promise<HaiResult<Article>> {
  const article = { ...input, id: core.id.generate(), createdAt: new Date().toISOString() }
  // DB 列映射由应用定义；create 返回 ExecuteResult，不是 Article。
  const result = await articles.create({ id: article.id, title: article.title, created_at: article.createdAt })
  if (!result.success) return result
  return ok(article)
}

export async function listArticles(params: ListParams): Promise<HaiResult<PaginatedResult<Article>>> {
  return articles.findPage({ pagination: params })
}
```

### 服务层规则

- 应用领域操作沿现有契约返回 `HaiResult<T>`；纯 getter、工具、工厂与流按实际签名，不统一改造
- 写操作需 `debug`（进入）+ `info`（成功）日志
- 读操作仅 `debug` 日志
- 禁止 `console.log`，使用 `core.logger`
- 服务层不处理 HTTP 响应格式，只返回 HaiResult

## 创建数据模型

### Schema 定义（SQL DDL）

沿用当前应用的迁移/建表入口；仅开发初始化用的简单 SvelteKit 示例可在 `$lib/server/init.ts` 建表。不要把本例 SQLite DDL 无修改复制到 PostgreSQL/MySQL，迁移与索引需匹配实际数据库。

```typescript
const BUSINESS_SCHEMA = `
CREATE TABLE IF NOT EXISTS articles (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content TEXT,
  author_id TEXT NOT NULL,
  status TEXT DEFAULT 'draft',
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_articles_author_id ON articles(author_id);
CREATE INDEX IF NOT EXISTS idx_articles_status ON articles(status);
`
```

### 类型定义

```typescript
// 在服务文件或独立类型文件中定义
interface Article {
  id: string
  title: string
  content: string | null
  authorId: string
  status: 'draft' | 'published' | 'archived'
  createdAt: string
  updatedAt: string
}

interface CreateArticleInput {
  title: string
  content?: string
  authorId: string
}
```

## 创建业务组件

业务组件放在 `$lib/components/`，通用 UI 使用 `@h-ai/ui`。

```svelte
<!-- src/lib/components/ArticleCard.svelte -->
<script lang="ts">
  import type { Article } from '$lib/types/article'
  import * as m from '$lib/paraglide/messages.js'

  let { article, onEdit }: { article: Article, onEdit?: () => void } = $props()
</script>

<div class="card bg-base-100 shadow">
  <div class="card-body">
    <h2 class="card-title">{article.title}</h2>
    <p>{article.content ?? ''}</p>
    {#if onEdit}
      <div class="card-actions justify-end">
        <button class="btn btn-primary" onclick={onEdit}>{m.article_edit()}</button>
      </div>
    {/if}
  </div>
</div>
```

`Article` 放在客户端可用的共享类型位置，`article_edit` 在中英日文 messages 中新增后由项目生成；实际已安装 `@h-ai/ui` 有适用 Card/Button 时使用其公开组件和 Props，本例只展示业务 props、事件与文案边界。

### 组件规则

- 使用 Svelte 5 Runes 语法（`$props()`、`$state()`、`$derived()`、`$effect()`）
- 事件回调通过 props 传入（非 `createEventDispatcher`）
- 样式使用 TailwindCSS 4 + DaisyUI 5 class
- 通用 UI 组件优先使用 `@h-ai/ui`，不重复实现
- i18n 文本使用 `$lib/paraglide/messages.js` 中的 key

## 初始化顺序

CLI 项目统一忽略 `**/data/`，含根目录、workspace 子目录及 `.gitkeep`；数据库/上传文件不入 Git。Fullstack 的服务端 config 始终保留 core、serv、db、cache、iam、storage、audit、reach、ai、vecdb、payment、scheduler、deploy 共 13 份 YAML，未启用模块整份注释，可选字段及替代 Provider 按块注释，`.env.example` 也保留未启用模块的注释凭据。

H5 所有页面继承根布局的 header/footer；认证页隐藏底部导航，业务页面使用 `@h-ai/ui` 的 `BottomNav` 与移动端壳样式，导航启用 centered/maxWidth/safeArea，与页面同宽。

扩展这些配置前先核对当前安装版本及模块公开 Schema。在服务端 workspace 安装需要的依赖，在 `src/server/runtime.ts` 按依赖顺序校验/初始化、失败清理并反序关闭，再接入 contract/procedure。整份注释文件先去掉一层外层注释，可选块只开启需要的字段；切换 Provider 时替换旧连接块并保持名称唯一。配置文件存在不代表模块已启用。crypto.init() 无配置参数，datapipe 无生命周期，不增加虚构的 YAML。

环境变量叶子覆盖只遍历 YAML 已有路径，注释键须先取消注释，或用 HAI_DB/HAI_CACHE 等整体对象覆盖。camelCase 不拆词，例如 connectTimeout → HAI_CACHE_CONNECTTIMEOUT；密钥仅写入被忽略的环境文件或部署环境。语言列表在前端/shared 的 project.inlang/settings.json 中同步，core 只配置 defaultLocale。

Fullstack 的公共 `.env` 启用当前模块的运行参数，YAML 显式引用同名变量并保留本地回退；未启用模块与可选字段保留注释。环境文件的用途、加载优先级及私有值保存方式遵守 [环境配置规范](development.md#环境配置)，模块仍按公开 Schema 校验。

复用当前应用服务端启动/关闭入口；SvelteKit 通常为 `$lib/server/init.ts`，API/fullstack 使用已有 service/serv 初始化文件。下列是启用 IAM 时的常见依赖关系，不要求无关应用安装这些模块：

```
core.init() → crypto.init() → reldb.init() → cache.init() → iam.init() → createBusinessTables()
```

新增生命周期模块时按实际配置/依赖添加初始化、检查结果并反序关闭；关键依赖失败停止启动并清理已创建资源。core 无 close、datapipe 为纯函数、serv/kit/contract 为工厂或适配器，不能虚构 init/close。

## 容器交付

仅涉及容器构建/部署变更时使用本节；普通功能开发无需每次启动镜像。

- 保持脚手架 `images/*.Dockerfile`、`.dockerignore` 与 `package.json` 中 `build` / `compile` 命令与 hai.build.json同步。保留 `.gitattributes`，让 `.dockerignore`、Dockerfile 和 POSIX 脚本统一使用 LF，保持跨平台输入一致并确保 Linux 脚本可运行。
- Podman 多阶段构建保持 `--format docker --jobs=1`；前者保留 `HEALTHCHECK`，后者避免部分 Podman 5.8/Buildah 组合在并行 stage 构建时触发内部 semaphore panic。
- 先复制 lockfile、workspace 配置与构建器准备的 `images/dependencies/` 安装输入执行 `pnpm fetch --ignore-scripts`，再复制源码并离线安装、构建；fetch、安装和 `pnpm pm deploy` 共用 pnpm store，安装与生产依赖整理共用 SQLite 驱动缓存，避免重复下载，兼容新增 workspace、本地归档与补丁。
- 默认容器面向 CPU 运行或远程 AI 服务，Dockerfile 设置 `ONNXRUNTIME_NODE_INSTALL_CUDA=skip`，跳过额外 CUDA/TensorRT 下载；使用宿主机 GPU 时定制镜像和运行参数，不修改本机 npm 或全局 AI/GPU 配置。
- SvelteKit 服务使用 Node adapter；纯 Vite 前端使用带 SPA fallback 的静态镜像；fullstack 的 Service 与 Web 分别交付，Service 直接监听且不增加转发进程。
- Web 的 `PUBLIC_*` 部署配置在容器启动时生成静态运行时配置，不作为 build arg 烘焙，确保同一镜像可跨环境部署。
- 配置文件随镜像提供，并允许只读挂载覆盖；密钥只通过环境变量或 `--env-file` 注入。
- 普通构建不携带 `data/`；持续写入使用运行期 volume；需要预置数据时显式维护独立 Dockerfile 和明确的 build context，不把本机数据加入默认镜像。
- 数据库和本地存储的持续写入必须落到 volume，不能依赖镜像可写层。
- 完成前实际启动镜像，验证健康端点、非 root 用户、环境变量覆盖、配置挂载和预置数据读取。

## 检查清单

- [ ] 需求已匹配框架现有能力/feature/配置，版本与契约已核验，自定义缺口有依据
- [ ] 新增/改变行为有意义的目标用例先失败、后通过；既有回归保持通过
- [ ] 适用安全边界已验证：非法输入、越权/租户隔离、失败结果及相关事务/幂等/清理
- [ ] 相关 API/页面改动已执行适用集成/E2E 流程
- [ ] 页面使用 Svelte 5 Runes 语法
- [ ] API 端点有权限守卫与输入校验
- [ ] 服务层和调用方遵循实际返回/抛错/流契约，不能以 HTTP 成功替代业务状态成功
- [ ] 日志级别正确（读 debug / 写 debug+info / 失败 warn|error）
- [ ] 禁止 `console.log`、`any`、硬编码密钥
- [ ] 用户可见文本走 i18n
- [ ] 涉及容器交付时，镜像启动及配置/环境变量/数据目录行为已验证
- [ ] 新增表有索引定义
- [ ] 组件优先使用 `@h-ai/ui`
- [ ] 按当前 scripts 运行适用 typecheck/lint/build/test/E2E 并报告未验证项

## 相关 Skills

- `hai-build`：项目架构与模块初始化顺序
- `hai-app-tests`：TDD 测试规范（Red 阶段详细指引）
- `hai-app-review`：代码审查规范（Refactor 阶段参照）
- `hai-kit`：SvelteKit 集成的完整 API
- `hai-ui`：UI 组件库使用
- `hai-reldb`：数据库操作详细 API
- `hai-iam`：认证与权限管理
