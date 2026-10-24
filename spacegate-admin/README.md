# spacegate-admin

## 简介

前后端分离工程，API 与 Web、移动端、桌面端共享接口类型和管理界面，可按实际业务调整。

## 核心功能清单

- API 服务、共享界面与按需生成的客户端。
- 用户管理、角色、审计和个人设置示例。
- 登录、主题、语言、新手引导和帮助中心。
- API/Web 独立 Dockerfile 与镜像归档。

## 使用入门

### 工程结构

- `packages/spacegate-admin-contract`：前后端共享 API 契约、Zod Schema 与类型
- `packages/spacegate-admin-serv`：后端 API Service（通过 `@h-ai/serv` 暴露最小 HTTP App 抽象，不直接暴露 Hono）
- `packages/spacegate-admin-shared`：跨端共享 Shell、主题、语言切换、API client 与共享 messages
- `apps/spacegate-admin-web`：Web 前端
- `apps/spacegate-admin-app`：移动端 App/H5 前端
- `apps/spacegate-admin-desktop`：桌面端前端
- `apps/spacegate-admin-miniapp`：小程序预留说明与本端 inlang messages（占位，不参与构建）

已启用前端：Web / App / Miniapp / Desktop

### 页面布局与路由

Web/App/Desktop 共享 AppShell 的 header/footer。新增页面放 shared/src/lib/pages/**/*.svelte，由 HashRouter 自动发现并按需加载；菜单只维护入口。地址使用 #/页面路径，支持刷新、查询串与浏览器前进后退；API 权限仍由服务端校验。

### 默认管理功能

以下描述可运行示例的初始业务，可按实际需求调整或移除，并同步代码、数据、鉴权、界面、测试和文档。

默认启用 IAM、reldb、cache、audit、storage 和 crypto。初始管理员可以管理用户（新增、编辑、禁用、删除、角色分配、头像上传）、查看 admin/user 角色及人数、维护角色说明、分页筛选审计日志；初始普通用户进入个人资料、头像、语言、主题和密码设置。默认密码修改后全部会话失效，需要重新登录；角色模型、访问范围和会话策略由业务需求决定。

CLI 已在根目录生成 `.env.local`，其中 `HAI_ADMIN_DEFAULT_PASSWORD` 是每个项目独立生成的随机强密码。直接运行 `dev` / `start` 即可，首次登录使用用户名 `admin` 和该密码；也可通过同名环境变量覆盖。密码须满足 `_iam.yml` 策略，启动失败会显示具体校验原因及错误码。`.env.local` 已被 Git 和容器构建忽略，`.env.example` 不含真实密码；部署时设置目标环境自己的密码。已有用户时不会重复创建管理员或重设密码。

本地开发默认 `PUBLIC_API_BASE=http://127.0.0.1:3000`，Vite 使用同源代理；部署时改为实际公网 API Origin，Android 模拟器访问宿主机时使用 `http://10.0.2.2:3000`。不要将密钥放入任何 `PUBLIC_` 或 `VITE_` 变量。

服务端版本化传输密钥环在创建时一次性写入被忽略的 `.env.local`，变量为 `HAI_CRYPTO_TRANSPORT_SERVERKEYRING`；运行时不生成密钥。部署由 Secret Manager 向所有节点分发同一受管 ring，配合共享 registry。私钥不进入公开配置、PUBLIC_/VITE_ 或日志。

所有业务 JSON 请求和响应默认通过 `@h-ai/crypto` transport 双向加密（含登录、刷新、头像上传）；前端统一使用共享 typed API client。Web 访问令牌仅存内存、刷新令牌由 HttpOnly Cookie 保存；受信原生端使用平台安全存储或内存。头像为随机路径只读图片资源，支持 PNG/JPEG/WebP，最大 512 KiB。健康检查和只读头像图片可直接访问。

应用显示名称、版本、环境、默认语言和日志设置来自后端 `config/_core.yml`，前端通过 `app.info` 获取安全元信息。开发默认 SQLite、本地存储与内存缓存；生产启动拒绝这些本地 Provider，必须将所有节点连接到同一共享数据库、Redis 与 S3。传输必须在所有节点注入相同受管版本化密钥环与共享 registry，无需会话粘性；重启读取原 ring，轮换先分发新旧版本再切 activeKeyId。生产需显式配置允许的 CORS Origin。业务提交后的审计写入采用最大努力，失败记录结构化错误日志，保留已完成业务的原始结果。

### 配置扩展

服务端 `packages/spacegate-admin-serv/config/` 始终保留 core、serv、db、cache、iam、storage、audit、reach、ai、vecdb、payment、scheduler、deploy 共 13 份 YAML。已用模块保留工作默认值，未启用模块整份注释，可选字段和替代 Provider 按块注释；根 `.env.example` 也保留未启用模块的凭据示例。Reach 默认为开发控制台通知，生产按 `_reach.yml` 注释替换为 SMTP、短信或 API Provider，并保持名称唯一。

自动生成的 `.env` 包含已启用模块的环境参数；YAML 显式引用相同变量并保留本地回退。编辑环境参数优先修改对应 `.env`；未启用模块仍保留注释。

扩展时先在服务端 workspace 安装需要的包，在 `src/server/runtime.ts` 按模块公开 Schema 校验配置、初始化并反序关闭，再接入 contract/procedure 和界面。整份注释文件先去掉一层外层注释，可选块继续按需开启；配置文件存在不代表业务功能已接入。`crypto.init()` 无配置参数，datapipe 为纯函数，不生成虚构 YAML。

环境变量只覆盖 YAML 已有路径，注释键须先启用，或用 `HAI_DB` / `HAI_CACHE` 等整体对象覆盖。camelCase 不拆词，例如 `connectTimeout` 对应 `HAI_CACHE_CONNECTTIMEOUT`。修改配置后重启服务；语言列表在各前端/shared 的 `project.inlang/settings.json` 中同步。`**/data/` 忽略任意层级的数据目录及 `.gitkeep`，运行数据不入 Git，运行时自动创建目录。

### 分环境配置

运行参数在 `.env.example` 中逐项说明用途、枚举和范围。`.env` 为公共基础，`.env.dev` / `.env.test` / `.env.prod` 为环境覆盖，均提交 Git；`.env.local` 为本机私有覆盖，不提交 Git。优先级为进程环境 > `.env.local` > 环境文件 > `.env`。YAML 显式引用 `HAI_<配置名>_<路径>` 并保留本地回退，camelCase 不拆词。完整命令、加载优先级和 Provider 切换见 [环境配置](.agents/skills/hai-app-create/development.md#环境配置)。

生产 .env.prod 按已启用模块选择共享 PostgreSQL（明确选择 MySQL 时保留）、Redis、S3、Qdrant，默认使用 Compose 内部服务名 db/cache/storage/vecdb。密码、S3 凭据、模型 Key 与受保护 Qdrant 的 Key 只由私有环境注入；容器外运行覆盖内部连接地址。生产 Reach 必须注入真实 HAI_REACH_PROVIDERS JSON，公共空数组会拒绝启动。AI 自动补全 db+vecdb 依赖，并在二者就绪后初始化，失败逆序清理。

| 用途 | 命令 |
| --- | --- |
| 开发 / 测试模式 / 生产模式预览 | `pnpm dev` / `dev:test` / `dev:prod` |
| 类型检查（check 为别名） | `pnpm typecheck` |
| 检查 / 自动修复格式 | `pnpm lint` / `lint:fix` |
| 源码构建 / 镜像构建 | `pnpm build` / `pnpm build --target docker` |
| 单元测试 / 监听 / 浏览器验收 | `pnpm test` / `test:watch` / `test:e2e` |
| Node 服务按环境启动（构建后） | `pnpm start` / `start:test` / `start:prod` |

Vite 客户端使用 `preview` 预览静态产物，Node 服务使用 `start`；纯 contract/shared 库只提供适用的构建和质量命令。API workspace 的环境文件位于 `apps/<name>-service`，fullstack 位于工程根。

### 开发与构建

```bash
pnpm install
pnpm dev
pnpm typecheck
pnpm build
pnpm test
pnpm test:e2e
pnpm build --target desktop --platform windows # Windows 主机；Linux/macOS 在对应主机构建
```

Tauri 需要目标系统的 Rust target、SDK 和打包工具。测试产生的临时工程、截图、报告、容器、镜像与进程应在 finally/teardown 中自动清理，失败也执行。

### 构建与部署

创建时默认包含部署工具；不需要时使用 hai create --no-deploy。
每个镜像对应 images/ 中一个 Dockerfile，hai.build.json 用 dockerfile 指定文件。
下载、安装和整理生产依赖复用同一份包缓存与 SQLite 驱动缓存，避免重复下载。
默认容器用于 CPU 运行或连接远程 AI 服务，不额外下载 GPU 运行库；需要使用部署机器上的 GPU 时，自行定制镜像和容器运行参数。

```sh
pnpm build
pnpm build --target docker
# 构建和部署一次完成
pnpm exec hai deploy packages/spacegate-admin-serv --env test --build
# 复用已有镜像；只生成完整离线包及跨平台部署脚本
pnpm exec hai deploy packages/spacegate-admin-serv --env prod --bundle-only
# 只更新前端（工程存在 web 镜像时）
pnpm build --target docker --images web
pnpm exec hai deploy packages/spacegate-admin-serv --env prod --services web
```

构建不读取部署环境；产物位于 .hai/build/docker/<buildId>，用 --artifact-dir 精确选择。
应用根 package.json.version 驱动版本，同版本允许重新构建和发布；清单记录独立构建编号与校验和。
完整离线包自动携带自建中间件镜像；准备中间件镜像需要构建机有可用 Podman/Docker，缺镜像时在打包阶段拉取。
服务器需要 Linux、Docker/Podman、Docker Compose v2 或 podman-compose 1.6.0 及以上、sh、tar、sha256sum、awk、curl、util-linux flock 和支持 flock 的文件系统，启动阶段不联网拉取。
固定 .deploy.lock 内核锁覆盖部署及恢复；禁止删除或替换，所有持有进程退出或重启后自动释放。强杀后须核对结果文件、current 和容器状态，再用新包恢复。
tar 旁的部署目录提供 deploy.ps1（Windows）和 deploy.sh（Linux/macOS），按已保存的 SSH 参数上传、导入、启动并验证 HTTP。
完全隔离时手工搬运 tar，独立目录解包后执行 sh apply.sh，或传入部署目录覆盖默认位置。
部分更新要求已有成功的完整部署，保留未选服务及数据卷；失败返回非零退出码，已有部署尝试恢复选中服务。
中间件连接齐全时使用 --skip-middleware 跳过配置向导。包中包含私有配置，不提交 Git。


### i18n 与 shared 协同

- shared：`packages/spacegate-admin-shared/project.inlang` + `messages/`，只放跨端共享组件文案。
- web/app/desktop：每个应用都有自己的 `project.inlang` + `messages/`，用于本端页面与产品文案。
- miniapp：当前是占位，但也有自己的 `project.inlang` + `messages/`，便于后续接入真实小程序框架。
- 新增 locale 时，shared 与所有启用前端的 `locales` 必须同步；语言切换器会同时更新 shared runtime 与宿主应用 runtime。
- `pnpm i18n:compile` 会统一编译 shared 与每个启用前端的 Paraglide 输出；各前端也可单独运行 `pnpm --filter spacegate-admin-web paraglide:compile` 等命令。

### 自定义建议

- 改全局 Shell/主题/语言切换：编辑 `packages/spacegate-admin-shared`。
- 改某个前端页面和文案：编辑共享 `packages/spacegate-admin-shared/src/lib/pages`、本端入口与 `apps/spacegate-admin-*/messages`。
- 改主题列表：调整脚手架配置中的 `fullstack.theme.themes`，或直接编辑各端 `src/app.css` 的 daisyUI `themes`。


### AI 辅助开发

从本项目根目录打开所用编码工具，先读取 [AGENTS.md](./AGENTS.md)。Claude Code 通过 [CLAUDE.md](./CLAUDE.md) 加载项目规则；GitHub Copilot 使用 [.github/copilot-instructions.md](./.github/copilot-instructions.md)，OpenCode 使用 AGENTS.md 和 [opencode.json](./opencode.json)。

生成的页面和业务规则是起点，可以按需求调整。把功能、权限和验收条件告诉 AI，再让它读取对应技能与当前安装版本的 API。

普通功能开发使用 [应用开发 Skill](./.agents/skills/hai-app-create/SKILL.md)。需求文档较多时，先用 [文档审查 Skill](./.agents/skills/hai-spec-review/SKILL.md) 找出缺失和矛盾，再用 [文档实施 Skill](./.agents/skills/hai-spec-implement/SKILL.md) 维护 docs/task-list.md 与 docs/tasks/，逐项实现、测试和交付。

已有项目可运行 hai init 补齐缺失的 AI 入口；它保留已有自定义内容。升级框架使用 [框架升级 Skill](./.agents/skills/hai-framework-sync/SKILL.md)，同时核对依赖、配置、代码和技能。

### 文档

- [hai Agent Framework](https://github.com/hai-series/hai-framework)
- [@h-ai/serv](./.agents/skills/hai-serv/SKILL.md)
- [@h-ai/ui](./.agents/skills/hai-ui/SKILL.md)

## 开发测试

```bash
pnpm typecheck
pnpm lint
pnpm build
pnpm test
pnpm test:e2e
```

## 许可证

Apache-2.0
