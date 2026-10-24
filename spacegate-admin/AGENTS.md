# AGENTS.md

Fullstack 多包工程入口。先读 README、根 workspace 脚本与相关 package/app；模块用法按需读取 `.agents/skills/*/SKILL.md` 及其引用。

## 需求与模板

- 业务行为以用户当前明确要求及其指定的需求/设计文档为准。本文件、README、skills、示例、生成代码及其测试中的模板默认行为是起点，不是不可变的业务约束；角色代码、权限模型、页面流程、字段、限额、存储方式与架构均可按实际需求调整。
- 需求与模板不同，审查时记录为模板适配项，不据此判定需求错误或阻塞审查，也不要求用户额外授权“覆盖模板”；实施时在已有授权范围内修改或移除冲突逻辑，并同步 contract、鉴权、数据迁移、界面、配置、测试、文档及本文件中的过期规则。
- 未说明的行为可参考现有实现并注明假设，不能升级为强制需求。只有业务要求自身矛盾、与已确认项目决策的优先级不明，或存在真实技术/安全障碍时才提出具体问题；已明确的规则不重复要求选择。
- 业务规则与技术契约分别判断：可用 API 以当前安装版本的公开类型为准；输入校验、服务端授权、资源隔离和密钥保护仍须落实。需求涉及替换技术方案时说明影响并同步迁移，不能用保留模板默认值代替解决问题。

## 行为契约

需求/设计文档审查或从文档生成代码时，读取 [hai-spec-review](.agents/skills/hai-spec-review/SKILL.md)，严格确认完备性、歧义、技术正确性与命名一致性；影响行为的问题未解决前不判定文档可生成。普通小改动不重审无关文档。

升级 hai-framework 到最新/指定版本、比较版本差异或同步框架 skills 时，先读 [框架升级与同步 Skill](.agents/skills/hai-framework-sync/SKILL.md)，执行版本发现、差异分析、用户选择/阻塞处理、迁移与验证；仅要求比较时不修改应用。

根据文档规划、实施、继续、返修或按已提交文档变更同步实现时，读取 [hai-spec-implement](.agents/skills/hai-spec-implement/SKILL.md)：维护 docs 清单与任务记录，比较已验收与目标文档 Git 快照，更新原任务或新建任务，每项独立分支 TDD、验证并本地合并；全部差异与整体验收通过后才推进基线。返修复用原任务与分支。仅规划不实施，指定任务先核验依赖。

普通新增、修改、修复任务开始前，先读 [应用开发 Skill](.agents/skills/hai-app-create/SKILL.md) 和 [开发规范与框架能力路由](.agents/skills/hai-app-create/development.md)；再按任务读取模块 Skill 及其相邻参考文件。架构/构建、审查、测试分别读取 hai-build、hai-app-review、hai-app-tests，不加载整个 skill 树。

- 对当前需求逐项检索 hai-framework 能力和已有实现，确认适用的公开 API 后优先复用；未安装模块先核对依赖与配置，不能因 Skill 已存在就假定已启用。框架不适用时说明具体限制与选用方案，不为覆盖清单引入无关模块。
- 按已安装版本的公开类型确认模块契约：生命周期模块按 init → use → close；core/serv/kit、纯函数/工厂与流按实际签名处理。返回 HaiResult 的操作先检查 success，不以 try/catch 代替判断；事务失败明确触发回滚。
- 边界输入做 schema 校验，服务端验证身份、租户与资源权限；SQL 动态值参数化，文件路径防逃逸。密钥不进入 PUBLIC_/VITE_ 变量、客户端代码和日志；敏感 token 不写 localStorage。
- 禁止 any、无说明的 as unknown as、console.log 和硬编码密钥；从包公开入口导入。注释中文、结构化日志英文并脱敏，用户文本走现有 i18n 并同步所有 locale。
- 完成时说明实际复用的框架能力、未采用的相关能力及原因、门禁结果；跨边界同步类型、配置、调用方、文档和测试。

- 修改前检索实现、调用方、测试和文档，优先复用现有 contract、procedure、共享组件与脚本。
- 跨包变更先明确受影响的 contract、serv、shared、前端及交付流程，再同步调用方；不必输出固定问卷。

## 初始架构

- 页面统一继承公共 header/footer；新增 SPA 页面放 `packages/spacegate-admin-shared/src/lib/pages/**/*.svelte`，由 `HashRouter` 自动发现。菜单只维护业务入口，不维护路由表。

以下为生成时的目录和分层；需求调整架构时同步相关包、脚本与说明，按调整后的实际结构工作。

| 路径 | 职责 |
| --- | --- |
| `packages/spacegate-admin-contract` | API contract、Zod schema 与共享类型 |
| `packages/spacegate-admin-serv` | 使用 `@h-ai/serv` 实现后端业务 API |
| `packages/spacegate-admin-shared` | 共享 UI Shell、主题、i18n 和 typed API client |
| `apps/spacegate-admin-web` | Svelte 5 + Vite Web 前端，按选择生成 |
| `apps/spacegate-admin-app` | Svelte 5 + Vite + Capacitor 移动端 SPA，按选择生成 |
| `apps/spacegate-admin-desktop` | Svelte 5 + Vite + Tauri v2 桌面端，按选择生成 |
| `apps/spacegate-admin-miniapp` | 选择 miniapp 时生成的预留说明与文案，当前不是可运行应用 |

- 前端负责 UI 与请求编排；后端业务放 serv，共享能力放 shared。保留当前 Vite SPA 架构时，不混入 SvelteKit load / route 结构；需要迁移时成套调整。
- 用户文本按 shared 跨端文案与各 app 专属文案分工，同步现有 locale。
- 本样板默认启用 IAM、reldb、cache、audit、storage 和 crypto，复用现有初始化与关闭顺序；新增模块前先检查依赖、配置与公开契约。
- 应用名称、版本、环境、日志与 locale 配置通过 core 统一获取；浏览器显示来自服务端 `app.info`，不另写硬编码副本或将私有配置注入客户端。

## 初始管理业务

这些是可运行示例的默认行为，按实际需求选择保留、修改或移除；不是 hai-framework 对所有应用的要求。

- 初始使用 `admin` / `user` 两种角色：管理员进入用户、角色和审计管理，普通用户进入个人设置。角色代码、数量、权限粒度及可访问页面由业务需求决定；更换模型时同步服务端授权、资源范围与各端入口。
- 初始角色页维护说明并显示人数，用户页分配角色；示例防止管理员自删除、自禁用、自降级及移除最后管理员。是否采用这些流程和保护由需求确定，保留的业务不变量需覆盖并发管理请求。
- 默认头像接受 PNG/JPEG/WebP，最大 512 KiB；普通用户更新自己的头像，管理员可替用户更新。格式、限额和操作主体可调整，contract、客户端和服务端保持一致；仍需验证文件内容及资源访问权限，storage key 由服务端生成。
- 默认用户与审计列表使用页码分页：`page >= 1`、`pageSize` 为 1–100、每页 20 条。分页方式、筛选和上限可按需求调整，同步 schema、procedure、store 和测试。
- 默认将语言与主题偏好保存到当前用户 metadata，审计采用操作字段和资源标识。偏好来源、审计字段、保留期及失败策略按需求确定，敏感数据按实际用途脱敏。

## 当前传输与安全契约

- JSON 业务请求与响应经 typed api-client 和 serv 的 crypto transport 加解密，包含登录与刷新；健康/就绪检查和公开头像二进制资源使用既有例外。新增 API 沿用当前管道；需求调整传输方案时同步客户端、服务端及测试，不能静默绕过仍生效的安全契约。
- Web 使用内存 access token 和 HttpOnly refresh Cookie；App 原生端装配 `createCapacitorTokenStorage`，浏览器预览沿用 Cookie；Desktop 原生端使用内存 TokenStorage，重启后重新登录。原生 body 刷新通道同时验证 `X-HAI-Token-Transport: body`、端对应的 `x-request-client` 与受信完整 Origin，刷新请求沿用这些头。调整会话方案时保持凭据保护和通道校验；敏感 token 不写 localStorage 或普通配置文件。
- 服务端依据实际权限模型验证每个受保护操作，个人操作绑定当前会话，代他人操作验证对应权限；不能只靠隐藏导航保护资源。SQL 动态值参数化，日志与审计不泄露密码、token 或密钥。

## 代码注释与示例

- 尽可能补充有信息量的中文 JSDoc：所有 export 的函数/类/方法、核心算法与业务流程、所有数据库表/列、接口/type/schema 及其字段都必须说明用途、单位、约束和关联关系。核心逻辑解释决策原因、边界、权限与失败路径，不逐行复述代码。
- 函数按实际契约写 @param、@returns，仅确实抛错时写 @throws；异步、HaiResult 与事务失败行为须说明。类型/表结构说明字段含义、可空性、默认值、主外键和唯一约束。
- 上述函数和结构的 JSDoc 均提供可理解的 @example：函数给调用及结果判断，结构给合法数据，表给典型行或查询示例。省略上下文要标明，不使用真实凭据。示例随实现同步，不能只堆叠没有信息量的注释。
- 调用及结构示例见 [.agents/skills/hai-app-create/development.md](.agents/skills/hai-app-create/development.md) 的 JSDoc 注释一节。

## 质量门禁

按 [分轮验证与集中修复](.agents/skills/hai-app-tests/validation.md) 收集一轮适用门禁及日志，汇总后集中修复、复测失败及受影响项；普通失败不中断独立检查，真实前置阻塞才暂停依赖项。跨包契约从根工作区验证：

```bash
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm test:e2e
```

发布或原生壳改动另检查 Capacitor/Tauri 配置、根脚本，并按目标平台执行 `pnpm compile`、`pnpm package`。浏览器测试不能代替原生包验证。

## 完成条件

- 同步受影响的 contract、serv procedure、shared client、各端调用方、README、messages、skills 和测试。
- 报告实际门禁结果、受影响 package/app、未执行项及原因；不得将跳过或失败写成通过。

## 优先 Skills

构建与验收：`hai-build`、`hai-app-review`、`hai-app-tests`。管理业务按需读取 `hai-iam`、`hai-reldb`、`hai-cache`、`hai-audit`、`hai-storage`、`hai-crypto`；配置与跨端读取 `hai-core`、`hai-ui`、`hai-serv`、`hai-api-contract`、`hai-api-client`、`hai-capacitor`。

前端页面先按 [hai-ui-design](.agents/skills/hai-ui-design/SKILL.md) 确定任务、布局、状态与组件映射，使用 [hai-ui](.agents/skills/hai-ui/SKILL.md) 实现，再由 [hai-ui-review](.agents/skills/hai-ui-review/SKILL.md) 检查真实页面。窄范围修改复用已有设计；按实际语言/主题/尺寸验证。

工作流技能按 hai-领域-动作 命名，能力使用类沿用领域名。需求、验收、发现与当前版本证据按 [工作交接契约](.agents/skills/hai-spec-implement/handoff.md) 传递，沿既有清单恢复任务。
