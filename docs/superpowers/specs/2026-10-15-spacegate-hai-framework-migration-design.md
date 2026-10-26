# SpaceGate Hai Framework Migration Design

## 意图

把现有 SpaceGate 管理面迁移到 `hai-framework` CLI 生成的前后端分离工程，统一使用 Hai 的 API contract、IAM、typed client 和多端管理工作区。Web、移动 App、Desktop 和小程序都要能完成当前管理后台的完整操作，登录同时保留现有账号密码能力并支持 LDAP。

## 现状与问题

当前管理面由三个独立边界组成：

- `spacegate/binary/admin-server` 是 Rust Axum 管理 API，直接读写文件或 Kubernetes 配置，并提供插件发现、实例健康和配置版本控制。
- `spacegate/sdk/admin-client` 是面向该 API 的 TypeScript SDK，维护 `X-Client-Version`、401 和 409 处理。
- `spacegate-admin-fe` 是 Vue 3 + Element Plus Web 管理端，页面覆盖概览、网关、路由、插件、AI 队列限流和实例。

登录目前是 `ak/sk` 加可选 JWT Cookie，不能使用 Hai IAM 的用户、角色、审计和 LDAP 能力；前端也只提供浏览器界面，移动、桌面和小程序没有共享的 contract/client 与管理业务层。

## 目标

1. 用 `hai-framework` CLI 的 `fullstack` 模板生成一个独立的 pnpm workspace，服务端和每个前端通过同一份 API contract 通信。
2. 保留 SpaceGate Rust 网关和配置后端作为数据平面/上游控制 API；将新的 Hai 服务作为认证、授权、contract 和多端聚合层，避免重写已经稳定的 Rust 配置协调逻辑。
3. 迁移当前管理后台的全部操作：网关配置、路由配置与重命名、插件实例、Wasm/AI Gateway 管理、实例发现/健康/重载、配置版本冲突处理和中英文界面。
4. Web、Capacitor App、Tauri Desktop 复用共享管理工作区、store、contract 和 typed client；小程序使用真实可运行的微信原生 TypeScript/WXML 工程，共享 contract、领域服务、数据模型和翻译资源。
5. 使用 Hai IAM 提供账号密码、角色/权限、审计、会话刷新和 LDAP 登录；LDAP 连接配置只来自服务端环境或 `_iam.yml`，敏感值不进入前端构建产物。

## 非目标

- 不在本次工作中重写 `spacegate` Rust 数据平面、插件 ABI、文件/Kubernetes 配置实现或 AI Gateway 运行时。
- 不改变现有 Rust admin-server 的公开 REST 路径和并发版本语义；新服务通过上游客户端适配现有路径。
- 不把小程序降级为 README 占位、H5 跳转或仅查询页面；但不引入 Taro/uni-app 等第二套跨端 UI 框架。
- 不在本次任务中部署生产环境、迁移已有用户数据库或提交/推送 Git 历史。

## 工程布局

在 `spacegate` 仓库的迁移分支中新增 `spacegate-admin/`，以父仓库的 Git 为唯一版本边界；CLI 生成时关闭嵌套 Git 初始化和依赖安装，随后由父仓库统一跟踪：

```text
spacegate-admin/
  apps/spacegate-admin-web/       # Svelte 5 + Vite Web
  apps/spacegate-admin-app/       # Capacitor App/H5
  apps/spacegate-admin-desktop/   # Tauri v2
  apps/spacegate-admin-miniapp/   # 微信原生 TS/WXML，小程序真实入口
  packages/spacegate-admin-contract/
  packages/spacegate-admin-shared/ # store、领域服务、i18n、共享 Svelte 工作区
  packages/spacegate-admin-serv/   # @h-ai/serv API service + IAM + 上游适配器
```

CLI 使用 `hai create spacegate-admin --type fullstack --frontends web,app,miniapp,desktop` 生成 Web/App/Desktop、contract、shared、service 和小程序占位目录。生成后只替换小程序占位并扩展 SpaceGate 业务模块；保留 CLI 生成的 `.agents/skills`、配置布局、脚本和验证入口。

本次已核对上游 `main` 的提交 `5a6c0eb3274bf955ddfd098d42124f09322c00b8`，CLI 版本为 `0.1.0-alpha.63`。实际生成前核验该版本的发布状态；使用同一版本的 CLI 和框架依赖，不将源码里的未发布能力冒充已发布能力。工程使用 CLI 要求的 pnpm，既有 Vue 工程的 npm 配置不变。最低 Node 版本遵循 CLI 的 `^22.22.2 || ^24.15.0 || >=26.0.0`。

## 服务端边界

`spacegate-admin-serv` 暴露面向前端的 typed procedures。每个 procedure 先经过 Hai IAM session/role 守卫，再调用 `spacegate-upstream-client`：

- `config.names`、`config.get`、`config.update`：网关配置和选择器。
- `gateway.get/upsert/delete`：网关监听器、后端等配置。
- `route.list/get/upsert/rename/delete`：路由及其完整规则。
- `plugin.list/get/upsert/delete`：插件实例、Wasm schema 和展示名称。
- `discovery.instances/health/reloadGateway/reloadGlobal`：实例状态和运行时重载。
- `aiQueue.schema/readme/rules/resolve`：保留现有 AI Gateway 管理接口。

上游适配器集中处理 REST base URL、`X-Client-Version`、`X-Server-Version`、401、409 和 JSON/文本错误；409 映射为 contract 的版本冲突错误，前端必须提示刷新后重试。版本来自每个客户端实际读取的配置快照，由请求/响应 contract 显式传递，不能使用一个服务端全局最新版本替所有用户提交。Rust 当前只在 GET 和 409 返回版本头；写成功后的版本更新必须按真实协议设计并用并发测试固定，不能通过自动重试绕过冲突。配置/插件变更必须保持 Rust admin-server 的现有同步行为，不在前端重复写文件或 Kubernetes。

新服务用独立的服务身份调用上游 `/auth/login`，服务端配置提供 `adminBaseUrl`、`adminAk` 和 `adminSk`。取得上游 JWT 后仅在服务端内存缓存，通过 Bearer header 调用管理 API；过期时重新登录，不向客户端暴露上游凭据，也不把 Hai IAM token 当成 Rust JWT。上游生产配置应同时启用现有 `--key` 和 `--sk`，端口仅在内部网络可达；本地未启用认证的开发模式必须显式配置，不能在认证失败后自动降级。AI Gateway URL 独立配置，所有转发只能发往服务端已配置的目标。

Contract 中的输入使用 Zod schema，资源名、路由名、插件 ID、Wasm URL 和查询参数在服务端再次校验。所有写 procedure 都声明资源范围和角色要求；普通用户不能调用管理写接口。上游请求不把 LDAP 密码、IAM token 或其他 secret 写入日志。

## 认证与 LDAP

服务端启用 `@h-ai/iam` 的 password 与 LDAP 两种登录策略。`config/_iam.yml` 保留完整注释配置，默认关闭 LDAP；部署时同时设置 `login.ldap: true`、LDAP URL、base DN、bind DN、`searchFilter`、`usernameAttribute` 和 `HAI_IAM_LDAP_BINDPASSWORD`。服务包显式依赖 `ldapts`，并让 IAM 在未注入测试工厂时创建默认客户端。

前端通过 `iam.publicConfig` 决定显示密码/LDAP 表单，登录错误使用统一 i18n。Web 使用框架默认的内存 access token + HttpOnly refresh Cookie；Capacitor 使用安全 TokenStorage；Desktop 使用内存 token，重启重新登录；小程序使用 `wx.request` 的适配器并只在内存保存 token，进程重启重新登录。任何 token、LDAP 密码、bind 凭据和上游管理密钥都不进入 `PUBLIC_`/`VITE_` 变量或小程序构建资源。

角色至少映射 `admin` 和 `user`：管理页面和写操作要求 `admin`，普通用户只可访问个人信息/会话接口。登录、退出、密码变更、LDAP 登录和管理写操作写入 Hai audit。

首个管理员沿用 CLI 的 bootstrap：从服务端 `HAI_ADMIN_DEFAULT_PASSWORD` 初始化本地 `admin`，已存在时不重设密码。保留模板自带的用户/角色管理；LDAP 首次登录同步为普通用户，由现有管理员显式授予 `admin`，不根据用户名或邮件域自动提权。默认关闭公开注册，不迁移或修改旧 `ak/sk` 凭据。

Web 与 service 分别构建和部署，默认由同一公网 origin 的反向代理将 API 路径转发给 service，以沿用 HttpOnly refresh Cookie；本地开发使用 CLI 生成的可信 Origin 配置。跨域独立域名、第三方 Cookie 和小程序原生会话通道必须逐项验证框架的真实 contract、Origin/CORS 和 token 传输能力；小程序不具备浏览器 Cookie/fetch 环境，需要显式实现请求适配和独立的受约束 token 通道。

## 前端迁移

共享层维护 gateway 选择器、加载状态、版本冲突、错误提示、语言和主题；共享业务组件保留 Web/App/Desktop 的管理布局。页面按现有 Vue 功能拆成：

- 概览：网关、路由、插件数量和实例健康。
- 网关：创建、编辑、删除和监听器/后端字段。
- 路由：列表、规则编辑、重命名、删除和插件绑定。
- 插件：插件实例、schema 表单、Wasm schema 预览、展示名称和删除。
- AI 队列：限流规则列表、创建/编辑、解析结果和插件说明。
- 实例：实例列表、健康、按网关/全局重载。

Web/App/Desktop 通过相同 store 和 contract 运行；平台差异只留在入口、token transport、窗口/安全存储和打包配置。小程序实现对应的 WXML 页面、导航、表单和列表，复用 contract 类型、service 调用、校验、i18n JSON 和错误码，使用 `wx.request`/`wx.showToast`/`wx.navigateTo` 等原生 API。小程序的页面覆盖同一功能集合，不能在服务端为它单独降级。

完整迁移的验收基线固定为本次检视的 `spacegate-admin-fe` 工作树及其 `components/config` 子模块，含原有未提交内容；外部同名 `spacegate-admin-front` 只作为参考，不覆盖实际子模块。必须保留监听器/TLS/Redis 连接池、路由匹配/改写/后端/MCP、各层插件绑定及优先级、JSON 导入导出、嵌套 schema 编辑、OCI/HTTP(S)/本地 Wasm 来源及认证头、插件刷新、AI 租户限流 CRUD/规则解析、实例健康和各范围重载。复杂编辑器采用各平台原生可用的表单和文本输入，不把 Monaco 的 DOM/Web Worker 当作小程序可用运行时。实施前逐页形成操作清单并用于回归，不能用通用 JSON 编辑框替代既有的所有专用交互。

本分支交付新工程、独立启动/部署入口和切换说明；旧 Vue 工程和当前部署入口继续可用。新工程验收通过后提供显式选择新镜像/路由的配置，本任务不执行生产切换或删除旧工程。新增容器/本地启动脚本应能独立启动新控制面并连接已有 SpaceGate/AI Gateway，而不隐式改动现有运行服务。

## 验证标准

1. CLI 生成工程的模板门禁通过，生成目录不含嵌套 Git，`.env`/运行数据被忽略。
2. 先执行 contract、service、shared 的 targeted typecheck/lint/test，再验证各端构建；小程序执行独立的 TypeScript 编译、页面清单和 `wx` adapter 测试。App/Desktop 原生打包和小程序开发者工具验收单独报告，浏览器预览不等于原生通过。
3. service 使用 mock upstream 覆盖：客户端版本首请求、服务端版本更新、409 冲突、401、文本错误、插件 query 编码和路由重命名。
4. IAM 覆盖密码登录、LDAP 成功/绑定失败/搜索失败、角色拒绝、会话刷新和 audit 写入；LDAP 测试通过注入的 `ldapClientFactory`，不依赖外部 LDAP 服务。
5. Playwright 覆盖 Web、App、Desktop 预览的登录、导航、概览和至少一个网关/路由写操作；小程序用 mock `wx` 的端到端业务流程覆盖登录、列表、写入、409 和登出。
6. Rust `spacegate-admin-server` 原有 targeted tests 保持通过；新增服务的集成测试不修改真实配置文件或 Kubernetes 集群。

## 实施决策与风险

- 采用 TypeScript Hai service 作为新控制面、Rust admin-server 作为上游，是迁移期间风险最小的边界；代价是部署时多一个服务和一次 HTTP hop。
- 小程序采用微信原生壳，避免把 Svelte DOM 运行时强行塞进小程序；代价是布局代码有一份平台实现，但业务 contract/store/校验/翻译保持共享。
- 第一阶段先完成 contract + upstream adapter + Web 的完整闭环，再按同一 contract 接入 App、Desktop 和小程序；每个阶段都有可运行验证，避免一次性复制所有页面后才发现接口不兼容。
- 上游 Rust admin-server 的 JWT/ak-sk 登录在新工程中只作为兼容迁移期入口；默认新入口使用 Hai IAM。旧 UI 和旧登录路径保留到切换完成，不在本分支直接删除。

## 仍留给实现者的选择

- 具体 contract procedure 命名、文件拆分和 UI 组件选型按 CLI 生成结构与现有 `@h-ai/ui` 组件决定。
- 小程序的 `appid`、项目目录配置和发布脚本使用占位值，并通过环境/`project.private.config.json` 覆盖。
- 上游 admin-server 的内部地址、端口、TLS 和 Kubernetes service name 只写入服务端配置，不在前端固定。
