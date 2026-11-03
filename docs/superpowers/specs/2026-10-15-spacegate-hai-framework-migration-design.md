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

`spacegate-admin` 是独立产品和独立 Git 仓库，当前仓库根目录位于工作区根目录下的 `spacegate-admin/`。项目最初位于 `spacegate/spacegate-admin/` 并由 `spacegate` 父仓库跟踪；2026-10-31 已将工作目录和 7 个相关历史提交拆分到独立仓库。独立仓库与 Rust SpaceGate、旧 Vue 管理端分别管理版本、CI、构建与部署。未经用户明确授权，不得在仍由父仓库跟踪的目录直接初始化嵌套 Git；拆分时按下述仓库迁移步骤审查工作树、历史和敏感文件：

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

最初设计检视上游 `main` 提交 `5a6c0eb3274bf955ddfd098d42124f09322c00b8` 的 CLI `0.1.0-alpha.63`；历史生成记录使用 `@h-ai/cli@0.1.0-alpha.68`，当前工程以实际 lockfile 和已安装包公开契约为准。拆分或实现时不将未发布源码能力冒充已安装能力。工程使用 CLI 要求的 pnpm，既有 Vue 工程的 npm 配置不变。最低 Node 版本遵循 CLI 的 `^22.22.2 || ^24.15.0 || >=26.0.0`。

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

新仓库交付完整工程、设计与验收资料、独立启动/部署入口和切换说明；旧 Vue 工程和当前部署入口继续可用。新工程验收通过后提供显式选择新镜像/路由的配置，本任务不执行生产切换或删除旧工程。新增容器/本地启动脚本应能独立启动新控制面并连接已有 SpaceGate/AI Gateway，而不隐式改动现有运行服务。

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


## 2026-10-29 修订：独立仓库与旧版功能对齐

### 权威基线和状态

本节与用户本轮明确要求优先于本文此前“父仓库唯一版本边界”和历史进度文件中的完成结论；其他未冲突的架构、安全与多端要求继续有效。行为基线是 `spacegate-admin-fe` 当前活动页面及 `components/config` 子模块，不能以外部 `spacegate-admin-front` 替代；旧实现缺陷不作为必须复刻的行为。逐项来源和页面清单见 `docs/superpowers/progress/2026-10-24-legacy-admin-operation-inventory.md`，实施中还须核对当前源码与工作树，清单中的建议不自动转为已验收事实。

本轮检视时 `spacegate` HEAD 为 `15cb9b9f7503808459013be779bf3dd0f1e72ab0`，旧后台 HEAD 为 `8acde7dff147affe290c727643676a9ef59b4218`，配置子模块 HEAD 为 `c41e108bc5c29e78b5b6fe0fe36b9022be87ab22`。旧后台父仓库 `.gitmodules` 与子模块指针有未提交变动，必须保留；新后台已有未提交的路由插件链修复，须计入评估。以上 SHA 不代表当前工作树全部内容的快照。历史 P0–P6、mock 和浏览器预览仅为当时的证据，不能代替本修订的逐项验收。

### R0 独立仓库交付与切换

1. 独立仓库包含 `apps/`、`packages/`、锁文件、workspace 配置、构建/测试脚本、镜像及部署配置、说明、必要的 Hai skills，以及本设计和旧版功能基线的可追溯副本。修正文档和脚本指向父目录或旧仓库的相对路径；跨仓库引用用清晰的版本或位置说明，不假设同一目录布局。检查 npm workspace/catalog、Docker build context、CI、原生壳、i18n 与生成脚本，在干净检出中独立安装、构建和运行测试。
2. `spacegate` 与 AI Gateway 保留为外部上游，通过服务端环境配置其地址和凭据；不可将真实凭据、`.env.local`、数据库、运行数据、`node_modules`、构建产物或测试产物带入仓库或历史。使用公开示例配置，验证 ignore 和容器构建边界。
3. 在单独位置准备可审查的拆分结果，核实提交范围、作者和历史及敏感信息后再决定是否保留目录历史。不得改写现有仓库历史、覆盖目录、删除父仓库文件、引入子模块或推送远端；上述切换动作及目标远端需用户明确授权。文档与源代码可以先在现有工作区修复。
4. 验收：从独立目录执行锁文件安装、定向类型检查/测试、Web 和 service 构建；部署配置不引用仓库外本地文件；老仓库/旧后台仍可使用。原生打包及真实服务联调按环境分别记录，不把静态扫描或 mock 当成独立安装或生产验收。

### R1 统一交互与配置安全

- Web/App/Desktop 使用共享业务逻辑和现有 Hai 视觉主题；小程序提供原生可操作表单，同一业务语义和 contract，不以整份 JSON 替代常用配置。响应式布局可不同，但不得漏掉操作、字段、验证、错误反馈或权限。
- 当前网关跨页面与深链接保持一致；切换网关须清除不属于新网关的路由选择。异步加载不覆盖其他资源的草稿；资源编辑器以打开时的读取版本写入，409 保留用户草稿、显示冲突并由用户显式刷新，不自动重试写入。刷新可能丢弃草稿时先确认。
- 新建/编辑的取消、关闭按钮、遮罩、Escape、浏览器返回以及切换会整体替换草稿的类型、模板、导入操作，均须在有未保存变更时提供明确的继续编辑或放弃确认；保存失败保留草稿，成功刷新相关列表并提示是否需要重载。删除提示资源名及可知影响，明确确认后操作；禁用和加载状态防止重复提交。
- JSON 是高级导入导出能力：支持复制、粘贴或文件导入导出及解析错误反馈；非法内容不可覆盖草稿或保存，导入后保留未知字段并进行资源结构验证。真实 TLS 密钥、Wasm 凭据及 token 不进入日志、错误信息或示例；配置本身若包含上游必需凭据，显示时掩码，并避免不必要的重复和泄露。导出需明确提示可能含敏感配置。
- 管理操作由服务端执行 admin 守卫；个人功能保持现有 IAM 权限。针对加载、空态、错误、无权限、保存中及成功均给出实际状态；各端中英文本一致，已有日语同步。保留必要的键盘、触摸和屏幕阅读器操作。

### R2 路由和后端

- 路由列表支持按名称、主机名、匹配及后端搜索，显示名称、HTTP/MCP 类型、主机、匹配摘要、后端、优先级和路由级插件数；支持新建、编辑、复制（草稿名加 `-copy`，重复时报错不覆盖）、确认删除和刷新。选定网关后加载数据；操作完成后更新列表。编辑名称改变时走现有 rename API；上游 rename 先创建后删除并非原子操作，失败时提示可能部分成功，重新读取并展示事实，禁止自动覆盖或重试。
- HTTP 路由支持名称、priority、hostnames、多规则和多匹配；匹配全部对应 `matches: null`。路径支持 Prefix/Exact/RegExp 与重写，方法支持 GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS，请求头支持 exact/reg_exp，查询参数支持 exact/regular。保留并编辑规则 timeout、timeout_mode 和 balance_policy；至少一条 HTTP 规则，删除最后一条须阻止并提示。
- 每条 HTTP 规则和每个后端都有独立插件链；MCP 路由和每个 MCP 后端也有自己的插件链。各链不互相覆盖，保留 priority 和数组顺序。后端支持 Host、K8sService、File，端口、协议、权重、超时、HTTP/2 降级；非 File 端口 1–65535，权重非负。MCP 至少一个后端。已有后端发现 API 的选择器重新核验并补齐，发现失败时允许手工输入且不能清除已填值。
- MCP 提供 streamable_http 和 legacy_sse 的路径、timeout_mode、session_affinity 与后端表单；路径和 HTTP 路径/重写值须符合上游要求。仅新建时可切换 HTTP/MCP；切换类型立即重置当前草稿并保留路由名。保留 API、静态文件和 K8s 快速示例；示例按钮立即覆盖当前表单草稿并清除无效 JSON，但不直接写服务端，最终仍由显式“保存”提交。关闭未保存草稿仍需确认。
- 路由编辑器沿用旧版标签分组：插件链页同时显示路由级插件和逐规则插件；匹配页只维护规则/匹配条件，后端页维护后端和后端插件。三个层级的数据仍分别写入 `route.plugins`、`rules[].plugins`、`rules[].backends[].plugins`；不能因改用 Hai UI 而把旧入口迁入其他标签或折叠路径。
- 验收：覆盖两个规则与多个后端分别添加、编辑、排序和删除插件，保存并重新读取仍位于原层级；HTTP/MCP 专用字段和未知字段保持完整；覆盖复制、改名、冲突、失败与未保存关闭。上轮恢复的入口仍需核验交互和 Wasm，不能仅凭一个 mock 用例标记整体完成。

### R3 原生插件、Wasm 定义与绑定

- 插件中心保留原生目录和 Wasm 扩展两个入口、搜索、插件元信息、实例列表和创建/编辑/删除。原生插件支持已有实例与新建实例、展示名称及 schema 表单/JSON；mono 唯一实例需可读取和编辑，不允许创建重复实例。删除提示引用影响；不存在的实例引用显示缺失，不能静默移除。
- 自定义 Wasm **定义**支持来源 OCI、HTTP(S)、file 或本地路径、schema 路径、名称/描述、阶段和优先级、拉取策略、默认配置、匹配规则、root/vm ID、缓存、VM 池、clusters/limits、OCI 认证与 HTTP 认证头。合法性、必填、范围和条件联动参考旧表单及真实上游契约；仅预览 schema 不应修改配置。现有上游绑定实例依赖复制基础来源及其认证字段；兼容现有运行时前不得擅自删除这些字段或承诺可完全去重，必须掩码并避免日志、审计及意外导出泄露。定义编辑后同步相关绑定的来源字段，保留每个绑定的运行时覆盖和显示信息；逐个写入失败时报告已成功和失败项，提供可重试/重新读取的路径，不伪称事务成功。
- 将 Wasm 定义绑定到 gateway、route、rule、backend 时，选择基础定义及默认配置、schema 表单或 YAML 对象，生成/复用资源专属的 named 绑定实例，并把该实例引用加入对应插件链；不能直接把基础定义当作独立可配置的绑定。保留已有 `binding_scope`、`binding_base_plugin`、`binding_owner`、`binding_config_mode` 和 `plugin_config/default_config` 等兼容字段及现有绑定 ID 命名，编辑已有数据不得重建或误覆盖其他资源。打开绑定详情可编辑优先级、展示名和运行时配置；展示名属于共享实例，界面须说明影响。YAML/JSON/XML 原有数据读取和编辑须保留语义；不支持的格式不得静默转换或丢弃。
- 插件链允许添加、查看详情、调整 int32 优先级、排序和移除；路由级同 code 重复按旧版阻止，其他层级按旧版允许。顺序是有意义的数组；后端第一项为最外层。支持鼠标排序或上下移动及键盘/触摸等效操作。
- 插件绑定选择必须保留旧版复用/创建完整路径：先选插件分类、在紧凑目录选择插件类型，然后在绑定步骤明确选择“复用已有插件实例”或“为当前资源新建实例”；复用只列该插件类型的 named 实例，新建进入专用实例配置。绑定优先级与保存/取消在绑定表单中。新建成功后回到原 gateway/route/rule/backend 插件链；取消、创建失败和版本冲突不得追加。Wasm 定义先进入当前资源专属绑定配置。
- 旧后台 `http://localhost:9080/routes?gatewayName=spacegate-admin` 实测路由编辑抽屉约 78vw、绑定插件面板约 31.3vw（2043px 视口下宽 1594px 与 640px）。新后台插件目录与绑定步骤应均接近旧面板，使用 `clamp(28rem, 32vw, 40rem)`，目录卡片按窄面板换行并纵向滚动，不扩到大面积 90vw。路由新建/编辑抽屉使用 `clamp(42rem, 78vw, 100rem)`。尺寸切换带过渡效果，验收等待动画稳定后比较。
- 点选已有原生或 Wasm 绑定后，在同一对话框展示插件/实例信息并编辑共享展示名、int32 优先级及运行时配置，统一显式保存/取消；Hai UI 提供控件风格，不替代旧版布局/操作。展示名修改需说明会影响共享实例。
- 插件实例写入和资源写入是两个独立上游操作，不能宣称原子性。创建/更新实例时提交所属资源草稿的快照版本；写成功后只有在写后读取的版本恰好等于该快照加一时，才允许将该版本用于同一编辑会话后续资源写入。若版本不等于快照加一、写后读取失败，或写操作结果不明，显示部分成功和版本不确定并要求重新读取，不能用其他页面的较新全局版本掩盖冲突。Rust 中间件先检查再递增版本且不是原子 compare-and-swap；此判定不能证明并发串行安全，必须记录限制并针对真实上游验证，不得声称强并发保障。资源保存失败或用户取消时，不自动删除已经创建的实例，告知部分成功并允许安全恢复；未修改的其他编辑草稿仍须维持各自快照。
- schema 表单覆盖本地 `$ref`、对象、数组、默认值、enum/oneOf 和合理的深层回退；非法 JSON/YAML、必填缺失及结构错误不得保存。基于旧版数据做兼容测试，不把 schema 读取成功或通用文本框当作表单验收。
- 验收：同一 Wasm 定义绑定两个不同资源，配置互不覆盖；编辑定义、绑定与展示名称后检查作用范围和保存数据；覆盖新增/已有实例、各层顺序、冲突、取消、部分成功和敏感字段处理。

### R4 网关、AI 队列、概览与实例

- 网关覆盖创建、编辑、复制/删除和影响提示、监听器与 TLS、Redis URL 和连接池参数、日志/语言/开关、网关插件链、JSON 导入导出和 reload 提示；编辑已有网关不得通过 JSON 意外更名或覆盖未展示字段。
- AI 队列除租户规则筛选、CRUD、TTL、规则解析及说明外，须补旧版 Wasm 宿主配置：来源 URL、服务集群地址、失败策略、VM 池和按 AI Gateway schema 编辑 `plugin_config`；schema/readme 失败分别提示，保存仍遵守配置版本与重载语义，保留未知字段和其他 clusters。租户限流按 AI Gateway API 独立处理，不套用 Rust 配置版本；校验 tenant、RPS、burst、cost、TTL，展示解析命中与候选而非仅原始 JSON。
- 概览核验四项指标含义、当前网关摘要、实例健康、路由概览和各入口；实例页区分在线、离线、未知，支持复制 ID、网关/全局/路由级重载及目标选择。重载是版本化写操作，冲突不自动重试，不应将旧路由用于新网关。
- 验收：逐项使用旧操作清单的全局、概览、网关、AI、实例条目核对全部目标端；标记通过、差异、不可测及对应证据，未执行项目不标完成。

### R5 实施顺序与验收记录

按依赖推进：(1) 固定旧版逐项基线和工作树状态；(2) 先修插件选择/绑定和版本传播，再补路由编辑器；(3) 补网关与 AI 宿主配置；(4) 逐项核验概览、实例、i18n、权限及小程序原生表单；(5) 在独立目录验证拆分可行性和文档/CI；(6) 整体回归并报告原生和真实上游尚未验证的边界。每项以真实可观察行为建立测试，不修改真实业务数据；历史测试及上次局部修复不自动视为本轮验收。任何未确认的依赖或部分成功都写入既有进度记录，不提前宣布全功能完成。
