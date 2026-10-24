# hai 应用开发决策

日常新增、修改和修复功能时读取本文件；专项模块 API 只读取下表匹配的 skill。这里的“应用尽用”是让每项真实需求优先使用适用框架能力，并检查已有 feature、配置和扩展点；不要求所有项目安装或启用全部模块。

## 需求与模板的关系

业务规则以用户当前明确要求和指定的需求/设计文档为准。模板 AGENTS、README、skills、示例、生成逻辑和测试提供初始方案，不固定角色/权限、流程、字段、限额、偏好存储或部署架构。需求明确时直接在已授权范围内适配，不为保留模板而修改需求或重复要求选择；仅审查时列出适配项，不写业务代码。

实现证据用于确认当前行为，不能推翻目标业务规则。同步修改冲突的 schema、服务端授权、初始化/种子数据、迁移、UI、配置、测试与过期规则；未改变的行为继续回归。API 是否可用由安装版本的公开契约决定，服务端授权、资源隔离、输入校验和凭据保护仍需满足；确有不可行或安全问题时举证并给出替代方案。

## 先确认应用与版本

- 新手引导与帮助中心使用 `@h-ai/ui/guide`，按 [UI 引导指南](../hai-ui/guide.md) 接入。fullstack 的共享管理界面已有默认帮助入口；业务内容放共享 guide-store，使用应用 i18n 和稳定 data-guide 锚点，不重复实现浮层。

- 读取当前 `AGENTS.md`、README、`package.json`、lockfile、初始化入口和已有相似功能；用 `rg` 查实现、引用、测试。保留已有改动，沿用项目分层、命名、ESLint、i18n 与测试入口。
- 确认架构：SvelteKit 使用 `kit` 和文件路由；API workspace 使用 contract/service；fullstack 使用 contract/serv/shared 与所选前端；Vite SPA/原生端通过 typed API 客户端调用服务。不要把服务端目录、DB、密钥或 Node 初始化复制到浏览器。
- 独立 mobile 模板选择 IAM/crypto 时包含前端 crypto，默认启用 typed client 加密 transport；无 crypto 的客户端保留明确的明文 API 模式。只有匹配的明文服务才设置 `PUBLIC_API_TRANSPORT=off`；`PUBLIC_API_KEY_EXCHANGE_PATH` 只覆盖公开协商子路径。握手失败不降级，关闭时先销毁 client transport，再关闭自身初始化的 crypto；服务端 ring 私钥不进入客户端。
- 从所属 workspace 的依赖与 lockfile 确认 `@h-ai/*` 实际解析版本和 npm/local 模式。模板 skill 提供导航，**当前安装包的 `package.json.exports`、公开 `.d.ts` 与随包文档决定可用 API**；本地框架源码只在版本匹配或显式联调时作实现证据。
- 若 skill 与安装版本不符，以公开类型和实际行为核验。先使用已支持能力；任务要求升级时按 [hai-framework-sync](../hai-framework-sync/SKILL.md) 发现目标、比较差异并完成迁移验证。不能直接复制未发布源码的 API，也不能为绕过差异添加虚构兼容层。

### 页面与布局

页面共用根布局的 header/footer。独立移动应用的页面放在 `src/lib/pages/**/*.svelte`，Fullstack 放在 `packages/<name>-shared/src/lib/pages/**/*.svelte`，由 `HashRouter` 和 `import.meta.glob` 自动发现；新增页面无需修改路由表，菜单按业务配置入口。SvelteKit 保留 `src/routes/**/+page.svelte` 自动文件路由、服务端鉴权和刷新定位。

### 环境配置

应用运行参数集中在各项目的环境文件。公共文件只保存可公开的值，凭据写入本机覆盖或由部署平台注入。

| 文件           | 用途                                                   | Git    |
| -------------- | ------------------------------------------------------ | ------ |
| `.env.example` | 所有可用字段的示例、用途、枚举和范围；可选字段保留注释 | 提交   |
| `.env`         | 所有环境共用的公共基础配置                             | 提交   |
| `.env.dev`     | 开发环境公共覆盖                                       | 提交   |
| `.env.test`    | 测试环境公共覆盖                                       | 提交   |
| `.env.prod`    | 生产环境公共覆盖                                       | 提交   |
| `.env.local`   | 当前机器的私有配置，覆盖公共配置                       | 不提交 |

YAML 使用 `${HAI_DB_TYPE:sqlite}` 等显式引用，冒号后是未配置时的本地默认值。应用仍通过 `core.config.validate(name, ModuleConfigSchema)` 和模块初始化校验类型、枚举、范围和必填项；插值本身不能代替校验。只开启实际使用的模块，未启用模块的整份 YAML 保留注释。

生产禁止使用进程内或节点本地状态。CLI 的 `.env.prod` 按已启用模块选择 PostgreSQL（保留明确选择的 MySQL）、Redis、S3、Qdrant，默认 Compose DNS 为 `db/cache/storage/vecdb`，容器外运行用私有环境覆盖连接地址。必须私有注入密码、S3/模型凭据及受保护 Qdrant 的 API Key。AI 硬依赖 reldb+vecdb，按数据库→向量库→AI 初始化，失败逆序释放并检查关闭结果，清理失败保留重试义务。生产 Reach 必须私有注入真实 `HAI_REACH_PROVIDERS` JSON；公共空数组有意拒绝启动，不能沿用 console。`hai add ai` 补包与配置，但现有业务入口仍须接入相同生命周期。

#### 命名和类型

- `HAI_<配置名>_<字段路径>`：`_db.yml` 的 `type` 对应 `HAI_DB_TYPE`；`_ai.yml` 的 `llm.apiKey` 对应 `HAI_AI_LLM_APIKEY`。camelCase 不拆词，数组使用从零开始的索引，如 `HAI_AI_AUDIO_MODELS_0_BASEURL`。
- 框架配置统一采用路径命名，如 `HAI_REACH_PROVIDERS_0_PASS`。第三方 SDK 自身读取的 `OPENAI_API_KEY` 等外部变量保留厂商约定；应用 YAML 使用框架标准名。
- 完整变量引用按 YAML 标量还原类型：布尔用 `true/false`，数字不带单位；要保留看似数字/布尔的字符串，在环境值中保留 YAML 引号，例如 dotenv 中 `HAI_AI_LLM_APIKEY='"123456"'`。空字符串保持为空字符串。
- `HAI_DB` 等父级变量可用 JSON/YAML 整体替换对象，优先于子级变量；非必要不整体覆盖，避免遗漏字段。
- `PUBLIC_` / `VITE_` 只用于浏览器可见的 API 地址和非敏感参数。业务正文、模板 ID、表名/列名及数据结构留在代码/YAML；环境参数不能改变固定表结构契约。

#### 加载与启动

| 入口             | 开发       | 测试环境                 | 生产环境                                      |
| ---------------- | ---------- | ------------------------ | --------------------------------------------- |
| Vite / SvelteKit | `pnpm dev` | `pnpm dev:test`          | `pnpm dev:prod` 预览；build 不选择环境 |
| Node service     | `pnpm dev` | 构建后 `pnpm start:test` | 构建后 `pnpm start:prod`                      |

所有入口优先级一致：**进程环境 > `.env.local` > `.env.<dev|test|prod>` > `.env`**。Vite 配置在 Node 阶段通过 `core.config.loadEnvFiles({ mode, envDir })` 读取，检查 `success` 后注入进程；`development`/`production` 分别映射到 `dev`/`prod`。Node 服务按 `.env`、环境文件、`.env.local` 的顺序传入 `--env-file`，后者覆盖前者，已有进程环境最高。`core.init` 只读取已注入的变量，不自行读取 dotenv。

托管 `test:e2e` 是明确的隔离测试入口：服务子进程使用本轮绝对数据目录和完整模块配置，不继承 ambient 模块变量，也不读取用户 `.env.local`。Svelte 仅在 `HAI_E2E=1` 的测试分支跳过 dotenv；API 测试直接执行已构建产物。普通开发与生产的上述优先级和共享后端守卫保持不变，transport 使用测试独立可信 ring 与实际 registry。

`prod` 是文件模式名，对应 `HAI_CORE_ENV=production`；环境枚举为 `development | test | production | staging`。SvelteKit adapter-node 产物使用 `node --env-file=.env --env-file=.env.prod --env-file-if-exists=.env.local build/index.js`。文件路径相对进程工作目录，公共文件随项目提交；容器通过部署平台或 `--env-file` 注入目标环境值。浏览器只接收公开白名单，构建产物和镜像不包含 `.env.local`。

Fullstack 的环境文件在工程根；service 脚本通过 `../../.env*` 加载，Web/App/Desktop 的 Vite 自动 dotenv 加载关闭（`envDir: false`），开发和预览由 `core.config.loadEnvFiles` 从工程根读取，构建不读取部署环境文件。API workspace 的环境文件在 `apps/<name>-service/`。独立应用在各自根目录加载。静态 Web 通过 `runtime-config.js` 获取公开地址，部署环境覆盖无需重建镜像；原生包在复制静态资源前配置该入口。修改编译进客户端的默认值时仍需重新构建；fullstack 的公开应用元数据由 service `app.info` 返回。

#### 迁移已有项目

先把原来被忽略的 `.env` 私有值迁入 `.env.local`，再提交公共基础和环境文件。本机文件只保留需要覆盖的值，避免整份复制示例后固定开发环境。旧的 `HAI_AI_LLM_API_KEY` / `HAI_AI_LLM_BASE_URL` 改为 `HAI_AI_LLM_APIKEY` / `HAI_AI_LLM_BASEURL`；音频和图像厂商回退变量同样使用 `APIKEY` 后缀。Reach 和 deploy 凭据按当前 YAML 路径命名，以项目 `.env.example` 为准，同步迁移部署平台变量和本机文件。

原 `HAI_AI_ANTHROPIC_API_KEY` / `HAI_AI_ANTHROPIC_BASE_URL` 不再使用。默认协议为 `anthropic` 时改用标准 LLM 变量；混合模型池中有独立凭据或端点时，将 `apiKey` / `baseUrl` 配置在对应模型项。厂商回退 `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL` 保留，优先级以 AI 模块说明为准。

#### 常用变量及约束

| 变量                                                              | 说明与取值约束                                                                         |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `HAI_CORE_ENV`                                                    | `development \| test \| production \| staging`                                         |
| `HAI_CORE_LOGGING_LEVEL`                                          | `trace \| debug \| info \| warn \| error \| fatal`                                     |
| `HAI_CORE_LOGGING_FORMAT`                                         | `json \| pretty`                                                                       |
| `HAI_DB_TYPE`                                                     | `sqlite \| postgresql \| mysql`                                                        |
| `HAI_DB_DATABASE`                                                 | SQLite 文件路径或 `:memory:`；网络数据库为数据库名                                     |
| `HAI_DB_HOST` / `HAI_DB_PORT` / `HAI_DB_USER` / `HAI_DB_PASSWORD` | 主机、整数端口 1–65535、用户名、敏感密码；PostgreSQL 常用 5432，MySQL 常用 3306        |
| `HAI_DB_URL` / `HAI_DB_SSL`                                       | 非空连接 URL 优先于拆分字段；MySQL TLS 仅 false 或严格对象（{} 默认验证证书链和 DNS 身份）；PostgreSQL 接受其布尔/字符串模式或对象    |
| `HAI_CACHE_TYPE` / `HAI_CACHE_DB`                                 | `memory \| redis`；DB 为整数 0–15；memory 仅单进程使用                                 |
| `HAI_STORAGE_TYPE`                                                | `local \| s3`；local 要求 root，s3 要求 bucket、region、accessKeyId、secretAccessKey   |
| `HAI_SERV_SERVER_HOST` / `HAI_SERV_SERVER_PORT`                   | HTTP 主机和整数端口 1–65535；部署平台 HOST/PORT 可覆盖                                 |
| `HAI_AI_LLM_API` / `HAI_AI_LLM_TEMPERATURE`                       | `chat \| responses \| anthropic`；temperature 为数字 0–2                               |
| `HAI_AI_LLM_APIKEY` / `HAI_AI_LLM_BASEURL` / `HAI_AI_LLM_MODEL`   | 厂商密钥、服务 URL、实际可用模型名；启用时满足厂商要求                                 |
| `HAI_ADMIN_DEFAULT_PASSWORD`                                      | 首次创建管理员的密码，满足 IAM 密码策略；CLI 为 fullstack 本地 `.env.local` 生成随机值 |
| `PUBLIC_API_BASE`                                                 | 可公开的 API 地址，不得包含用户名、密码或 token                                        |

其余变量逐项见对应 `.env.example` 及模块公开 Schema；可选块启用后也应使用相同命名、显式引用与逐项说明。

#### 跨环境切换数据库

保持 `_db.yml` 不变，在公共 `.env.test` 设置：

```dotenv
HAI_DB_TYPE=sqlite
HAI_DB_DATABASE=:memory:
```

在公共 `.env.prod` 设置非敏感连接参数，密码由生产进程或目标机器 `.env.local` 注入：

```dotenv
HAI_DB_TYPE=postgresql
HAI_DB_DATABASE=my_app
HAI_DB_HOST=db.internal
HAI_DB_PORT=5432
HAI_DB_USER=my_app
# 在目标环境注入 HAI_DB_PASSWORD，或提供完整 HAI_DB_URL。
HAI_DB_SSL=require
```

切换到 MySQL 时相应设置 `HAI_DB_TYPE=mysql`、端口和凭据，并把 `HAI_DB_SSL=require` 改为严格 TLS 对象，例如 `HAI_DB_SSL='{}'`（默认校验证书链和 DNS 身份）；自签 CA 使用 `ca` PEM，mTLS 的 `cert` / `key` PEM 必须成对。私钥仅放被忽略的 `.env.local` 或目标进程环境。确保项目已安装所选 Provider 的驱动；数据库数据迁移另行执行，切换配置不会搬迁数据。缓存切到 Redis、存储切到 S3 时同样填写对应连接字段。

## CLI 安装与命令

CLI 的 npm 包名是 `@h-ai/cli`，安装后提供 `hai` 命令。使用脚手架前先全局安装并确认版本；PowerShell 中使用 `pnpm.cmd` 执行 pnpm 命令。

```bash
pnpm add -g @h-ai/cli
hai --version
hai --help

# 新建项目
hai create my-app
```

已有项目先进入项目根目录，再按需要执行 `hai add <module>`、`hai init` 或 `hai generate <type> <name>`。已安装 CLI 时，相同安装命令可更新到当前发布版本；需要指定版本时使用 `@h-ai/cli@<version>`。命令和模板以实际安装的 CLI 为准，不能把未发布的框架源码能力当作已安装包能力。

`hai generate page` 只写 SvelteKit 文件路由；`api` 按目标应用生成 SvelteKit 端点或 serv 的共享 contract/procedure 并接入已有聚合入口。API/fullstack 根自动选择唯一后端，多个候选用 `--workspace <包名或相对路径>`，也可进入目标 workspace。纯 Vite 不生成虚假文件路由；serv 不接受 `--output`，SvelteKit 输出限定 `src/routes` 内，定制 `kit.files` 时拒绝猜测；SvelteKit API 目标须已安装 `@h-ai/kit` 与 `zod`。不要删除模板 `@hai-generator` 聚合标记；自定义入口没有标记时手动沿现有架构接入。生成前统一校验路径和覆盖冲突，失败回滚本次修改；`--force` 不能覆盖已有业务领域注册。

CLI 用户消息通过现有中英字典输出。初始语言按非空的 `HAI_CORE_DEFAULTLOCALE` → `LC_ALL` → `LC_MESSAGES` → `LANG` 选择，`en`/`zh` 地区变体映射到现有语言，`C`/`POSIX` 为英文；未知语言保留 core 当前语言（默认中文）。CLI 内部命令响应 `core.i18n.setGlobalLocale('en-US')` 的同进程切换；包入口仍为命令行入口，不提供库调用 API。`hai generate` 省略类型/名称会交互，取消不写文件；生成源码的中文注释与用户名称不翻译。

`create` / `add` / `init` 的校验、选择项、模块/模板名称与描述、错误前缀及完成提示也使用当前 CLI locale；目录标签延迟求值。CLI 语言不修改生成应用的 `core.defaultLocale`。生成的中文 README、YAML/源码注释与外部工具原始错误保留原样；不要把这些正文机械替换成终端翻译。

## 将需求映射到现有能力

应用元信息只有一份配置来源：名称、版本、环境、默认语言与日志设置来自 `_core.yml`，使用 `core.config` 与 `core.logger`；Node 启动时不通过固定 `logging` 覆盖 YAML。SvelteKit 服务端 layout 和 fullstack `app.info` 仅公开必要元信息，不能下发整个 core 或模块配置。独立 Vite 客户端经 Node/core 加载 YAML 默认值，Web 部署通过 `runtime-config.js` 应用公开白名单覆盖；只修改部署覆盖无需重建，修改构建默认值则须重新构建。环境覆盖使用 `HAI_CORE_NAME`、`HAI_CORE_VERSION`、`HAI_CORE_ENV`、`HAI_CORE_LOGGING_LEVEL` 等既有规则；容器明确配置生产环境，不能用另一个状态来源代替 `core.env`。

开发前，对本次涉及的需求简述“需求 → 现有应用实现/框架能力 → 已安装版本与公开契约依据 → 最小业务补充/修改位置 → 成功、权限与失败验收”。简单任务可用几句话；跨模块任务用表格。查找顺序：已有应用实现 → 框架公开能力和配置 → 最小业务补充。

下表技能名用于能力导航；先确认 `.agents/skills/<name>/SKILL.md` 存在再读取。CLI 按应用类型/已选模块交付技能，缺目录不代表框架没有该能力：继续查已安装包的公开 exports/types；确需补充模块/技能时按当前 CLI 帮助和项目依赖策略处理，不能跳过需求匹配或猜测不存在的 API。

| 需求 | 优先检查的能力与 skill | 使用边界 |
| --- | --- | --- |
| 配置、日志、ID、结果/错误、i18n | `hai-core` | getter、logger、ID 是直接值；ID 不作为密钥 |
| 数据库查询、CRUD、事务、分页 | `hai-reldb` | 先检查现有 CRUD/Repository 与事务支持，不另建 DB 连接层 |
| 缓存、TTL、共享锁 | `hai-cache` | 区分 memory 与共享后端；单进程状态不能承担跨节点防重 |
| 注册、登录、会话、角色/权限 | `hai-iam` | 优先配置内置认证能力；受保护请求使用 iam.auth.verifyToken，serv 注入 IAM 时自动使用；批量用户与角色筛选复用公开 API；业务资源/租户授权仍由应用检查 |
| SvelteKit Hook、守卫、校验、响应 | `hai-kit` | 只用于 SvelteKit；沿用已有认证/transport 管道 |
| 共享输入输出和 HTTP contract | `hai-api-contract` | 扩展已有 contract 与 schema，不维护第二份业务路径 |
| API 运行时、内置 feature、route guard | `hai-serv` | 先检查内置 procedures/features 与 route auth/permission；不另写平行服务器 |
| 浏览器/原生 API、刷新会话 | `hai-api-client` | 已支持接口复用 typed client、认证与 transport；额外 fetch/刷新封装需有缺口依据 |
| 页面、表单、表格、弹窗、移动 UI | `hai-ui` | 按实际导出选择场景/组合/原子组件；应用补业务交互 |
| 文件上传、下载、元数据、预签名 URL | `hai-storage` | 检查已有 storage feature；大文件用流式读写，checksum 按实际字节计算，ifNoneMatch 拒覆盖；资源授权、大小/类型/路径限制不可省略 |
| 加密、签名、哈希 | `hai-crypto` | 复用公开 API 与密钥配置，不自创加密或硬编码密钥 |
| LLM、流式、Embedding、RAG、Agent/MCP | `hai-ai` | 先检查内置能力与 provider 配置，再补应用编排 |
| 文本清洗、分块、管线 | `hai-datapipe` | 纯函数无生命周期；分块不代表已经生成向量 |
| 向量集合与检索 | `hai-vecdb` | 后端、维度与过滤条件使用实际契约 |
| 邮件/短信/通知触达 | `hai-reach` | 模板已绑定 provider 时只传 template/to/vars；直接发送才指定 provider；投递状态不等于 HTTP 成功 |
| 支付、订单、订阅、回调 | `hai-payment` | 验签、金额/订单绑定、幂等与状态迁移仍需业务验收 |
| 操作审计 | `hai-audit` | 复用审计记录/查询与 PaginatedResult 元数据；应用决定脱敏、保留期及失败策略 |
| cron、持久任务、重试、手工触发 | `hai-scheduler` | 复用任务模型；明确取消、幂等、共享锁及可信代码边界 |
| 原生设备与安全 token 存储 | `hai-capacitor` | 仅在适用平台启用；桌面/小程序依当前项目适配 |

已有能力满足需求时使用它；已导入模块但仍手写其已有认证、缓存、上传、分页或 UI 功能，需要修正。确有缺口时记录缺失的具体能力、当前版本和最小补充方案，按需求增加依赖或应用代码；不为凑模块覆盖率增加服务和抽象。

独立 fullstack 默认后台的 Web/App/Desktop 共用管理组件、状态与 typed client。`@h-ai/serv/features/iam` 默认 procedures 可按需选用并与 contract 对齐；有业务缺口时覆写必要路由。主题的 DOM 更新和安全缓存使用 `@h-ai/ui/theme`（`applyTheme` / `getSavedTheme`），应用保留响应式状态与已编译主题白名单。初始 admin/user、管理员自我保护、头像限额、分页和审计策略都是应用示例，可按需求替换；实际权限、资源范围和写入一致性按目标业务落实。

Fullstack 创建时在被 Git 与容器构建忽略的根 `.env.local` 写入独立随机 `HAI_ADMIN_DEFAULT_PASSWORD`，`.env.example` 保留空占位。`dev` / `start` 自动读取；已有用户不重设密码。启动边界保留模块错误的具体说明与错误码，失败后反序清理。不能把生成的本地凭据复制到前端配置、日志、源码或镜像。

## 落实契约与安全边界

沿现有服务层按顺序落实 schema → 当前授权与操作权限 → 从资源 ID 查所有者/范围 → 需要时核验对应操作凭据 → 存储/通知等副作用 → HaiResult 与审计。框架身份授权不能替代业务资源范围，数据库实体/存储 key 不交给客户端决定。文件失败、checksum 不匹配、不可覆盖冲突及通知失败/延迟分别向调用方反馈，并按业务一致性要求事务回滚或补偿；不能静默返回成功。

按本次真实输入、资源和副作用选取下面检查，并把适用项变成可观察验收。不能用“已遵循安全规范”或 lint 通过替代行为证据。

| 边界 | 实现要求 | 验收证据 |
| --- | --- | --- |
| 输入与输出 | 外部输入在请求/contract 边界 schema 校验，限制长度、分页、大小与合法枚举；从公开类型推导共享结构 | 非法/超限输入被拒绝，未触发写入或外部副作用；输出符合 contract |
| 身份与资源 | 使用对应架构守卫；另查资源所有者、租户、角色/权限；不能信任客户端传来的 userId/tenantId | 未登录、缺权限、跨用户/租户访问失败；正常路径可用 |
| SQL、文件、URL、内容 | SQL 值参数化，动态标识符白名单；文件防路径逃逸；用户控制的服务端请求 URL 限制目的地；HTML/AI 内容按不可信输入处理 | 相关注入/逃逸/越界目的地被拒绝；正常输入仍可用 |
| 凭据与会话 | 密钥仅服务端配置/环境，`.env.example` 只放占位；PUBLIC/VITE 配置不含密钥。沿用 Cookie/内存 token，原生用安全 TokenStorage；检查 CORS、Cookie、CSRF 与现有管道 | 浏览器产物/日志无敏感内容；登录/退出/刷新及适用的跨域攻击失败路径可验证 |
| HaiResult 与错误 | HaiResult 型业务 API 先判断 `success`，服务层保留错误；HTTP 边界沿实际包络/状态契约并隐藏内部敏感细节。显式抛错、流迭代与 SvelteKit 控制流按各自契约 | 依赖失败不回退为空列表/成功消息；流失败与业务状态失败有反馈，用户输入得到保留 |
| 初始化与清理 | 只对有生命周期的模块按依赖 init → use → close；检查每次结果，关键依赖失败停止启动，清理已创建资源，关闭反序 | 初始化失败不能继续执行业务；关闭/取消后连接、监听器与任务退出 |
| 写入与重试 | 多步一致性用事务或明确补偿；重试/回调/任务定义幂等；跨节点业务状态使用共享存储 | 中途失败无不一致写入；重复请求/回调不产生重复副作用 |
| 风格与可维护性 | 禁止 `any`、无说明的 `as unknown as T`、`console.log`、硬编码密钥；注释中文、日志英文且脱敏；文案使用现有 i18n 并同步中英日文；遵循项目实际 lint 配置 | typecheck/lint 通过；人工核对脱敏、文本、分层与不受 lint 覆盖的规则 |

不把所有函数改成 HaiResult：纯工具、getter、工厂、客户端/流及控制流按实际签名。不把所有模块加上 init/close。应用服务根据已有业务契约返回 HaiResult，HTTP/UI 层做适当转换。

serv/oRPC 普通 procedure 返回的失败 HaiResult 可以位于 HTTP 200 包络，`error.httpStatus` 是错误元数据，不等于响应状态；输入 schema 校验、传输错误、路由中间件及 kit 的 Response 各有状态契约。验收同时检查响应体 `success/error.code` 和真实 HTTP 状态，不照搬另一架构的 401/500 断言，也不以 HTTP 200 判为业务成功。

## JSDoc 注释

所有导出函数、核心业务流程、表/列、接口/type/schema 及字段提供中文 JSDoc，说明用途、约束、单位、关联及失败行为，并包含 `@example`。函数用实际的 `@param` / `@returns`，仅会抛错的契约写 `@throws`；数据结构给合法数据，数据库表说明主外键、可空性、默认值、唯一约束和典型行。核心逻辑解释决策与边界，避免重复代码字面含义。

```ts
/**
 * 分页输入；page 从 1 开始，pageSize 为 1–100。
 * @example const query: PageQuery = { page: 1, pageSize: 20 }
 */
export interface PageQuery {
  /** 当前页，最小为 1。 */
  page: number
  /** 每页记录数，最大为 100。 */
  pageSize: number
}

/**
 * 将已校验的分页参数转换为数据库偏移量。
 * @param query - 已通过边界 Schema 校验的分页参数
 * @returns 从零开始的偏移量
 * @example pageOffset({ page: 2, pageSize: 20 }) // 20
 */
export function pageOffset(query: PageQuery): number {
  return (query.page - 1) * query.pageSize
}
```

## 验证与交付

测试临时工程、镜像、容器、辅助进程与数据须在 finally/teardown 自动清理，成功、失败和取消均执行；只清理本次创建的资源。日志、截图与报告保留到分析和交付结束再清理，不提交验收产物。

- 对新增/改变行为按 [hai-app-tests](../hai-app-tests/SKILL.md) 写有意义的测试；目标用例先因行为缺失失败，既有回归保持通过。UI/路由改动覆盖真实流程、异步失败与权限路径；纯文案/文档按影响验证。
- 从当前 `package.json` 选择实际脚本，优先 workspace filter，PowerShell 使用 `pnpm.cmd`。多项门禁按 [分轮验证与集中修复](../hai-app-tests/validation.md) 先脚本化收集结果和日志，再集中修复、复测失败及受影响项；普通失败不立即从头重跑。不能调用框架仓库私有命令替代应用检查。
- typecheck/lint 只覆盖部分质量规则；权限、事务、降级、真实 provider、浏览器、容器和原生交付各自需要适用行为证据。缺环境或密钥时报告未验证，不能用 mock 冒充真实集成。
- 完成说明：实际复用能力与自定义缺口、修改路径、已执行命令/行为结果、同步的 contract/调用方/配置/i18n/文档、未验证项及原因。无需无关模块的“覆盖率”清单。

`_core.yml` 只接受 name/version/env/defaultLocale 元数据与 logging；不提供 debug、全局ID设置、features或supportedLocales。业务开关放app.yml并用业务Schema校验；语言列表按实际翻译工程维护，日志用logging.level。core默认语言字段不会自动注册翻译。Logger导入保留LANG/LC_ALL/PYTHONIOENCODING，终端语言优先级不能被编码初始化覆盖。

框架环境目录从真实入口生成；JS/TS（含 Svelte script）中的 JSDoc 示例不构成新运行变量。Schema 字段注释仍生成用途说明，YAML/模板可选注释和字符串内生成代码引用继续保留；不能只在示例里写变量就声称已被实现消费。

公开导入以安装版本的 package.json.exports 为准：只使用已声明且有真实产物的根/子路径，不导入 src/dist 或内部 Provider/Deps。条件入口可以只提供配置和类型；IAM 浏览器入口不含服务端 iam 对象。Svelte 默认组件和明确的 selective/type-only 导出均合法，不能为形式统一扩大公共 API。
