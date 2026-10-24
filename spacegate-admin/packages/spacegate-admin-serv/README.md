# spacegate-admin-serv

## 简介

后端 API Service，基于 `@h-ai/serv` 将共享 contract 装配为最小 HTTP App 抽象，不在应用代码中直接暴露 Hono。

## 核心功能清单

- HTTP 服务、健康检查和共享 contract 的 procedure 实现。
- 用户、角色、审计与个人设置的示例业务。
- 模块配置、加密传输与服务端权限校验。

## 使用入门

### 默认功能与配置

以下是可运行示例的初始实现，角色/权限、管理流程、头像限额、分页、审计策略及部署方案均可按用户需求调整。需求与代码或模板规则不一致时，以需求为业务基线同步实现、测试与过期说明；实际框架 API 契约和服务端安全校验仍需满足。

- `config/_core.yml`：应用元信息（名称 / 版本 / 环境 / 语言），启动时由 `core.config` 加载。
- `config/_serv.yml`：服务端运行时设置（CORS 来源等）。
- `config/_db.yml` / `_cache.yml` / `_iam.yml` / `_storage.yml` / `_audit.yml` / `_reach.yml`：管理及开发通知配置，初始化前使用模块公开 Schema 验证。Reach 默认控制台 Provider，生产按注释替换为 SMTP/短信/API。
- `config/_ai.yml` / `_vecdb.yml` / `_payment.yml` / `_scheduler.yml` / `_deploy.yml`：保留未启用模块的完整注释示例；配置文件存在不代表模块已接入。先在本 workspace 安装对应依赖，再在 `src/server/runtime.ts` 按公开 Schema 校验、初始化并反序关闭，接入实际业务后按需取消注释。整份注释先去掉一层外层注释，可选字段继续按需开启。
- `src/index.ts`：进程入口，先 `core.init({ configDir: './config' })`，再初始化 reldb → cache → crypto → storage → reach（有配置时）→ iam → audit；启动失败及退出反序释放。
- 选择 AI 时自动安装 reldb/vecdb/AI，并在 reldb/cache 之后初始化 vecdb、在其余依赖之后初始化 AI；关闭时先释放 AI，最后释放 vecdb/关系库，关闭失败会返回错误并保留重试义务。生产环境必须使用共享后端，根 `.env.prod` 默认连接 Compose `db/cache/storage/vecdb`，凭据和真实 Reach Provider JSON 私有注入。
- `src/app-server.ts`：服务装配入口，挂载健康检查、OpenAPI、docs 与 procedures；应用名称 / 版本与 CORS 来源均取自上述配置。
- `src/server/procedures/app-procedures.ts`：app 领域 procedures，对应 contract 中的 `app.info` / `app.echo` 路由。
- `src/server/procedures/admin-procedures.ts`：认证、管理员用户/角色/审计管理和个人设置，使用 `.auth()` / `.role('admin')` 守卫。每个请求读取用户最新启用状态、角色和权限。
- 标准认证 procedures 复用 `@h-ai/serv/features/iam`，注册、登录方式、重置与协议由 `_iam.yml` 控制；登录、退出、修改密码追加业务审计。HTTP App 类型复用公开 `ServHttpApp`。
- `src/server/admin-service.ts`：角色分配、头像上传和审计编排，全部底层操作复用 hai 模块。初始使用 admin/user 两种角色；锁内重新鉴权，默认禁止管理员自删、自禁用、自降级，并补偿用户更新失败后的角色变更。业务规则可按需求替换，保留的不变量需验证并发与失败路径。
- `tests/app-server.test.ts`：真实加密 HTTP 测试，覆盖 Cookie/受信原生刷新、RBAC、旧会话权限、并发管理操作、补偿、头像安全、偏好、审计及会话失效。

CLI 已生成根目录 `.env.local`，`HAI_ADMIN_DEFAULT_PASSWORD` 为每个项目独立的随机强密码；`dev`/`start` 自动读取，首次登录使用用户名 `admin` 和该密码。仅在没有用户时创建管理员，不重设已有用户密码。密码不符合 `_iam.yml` 策略时，启动日志显示具体原因及错误码。`.env.local` 被 Git 和容器构建忽略，`.env.example` 不含真实密码；部署时设置目标环境自己的密码。所有业务 JSON 请求和响应强制加密；健康检查和随机路径的只读头像图片可直接访问。头像支持 PNG/JPEG/WebP，最大 512 KiB，校验内容特征和大小后由服务端生成 key，不能指定任意文件路径。

默认 SQLite + 本地 storage + memory cache 适合单实例，管理写入仅支持单写实例：进程内保护无过期时间，共享 cache 锁定期续租，丢失锁或释放失败会记录错误。多实例读取可使用共享数据库、Redis cache 和 S3 storage，但管理写请求必须统一路由到同一写实例；仅更换 Redis 不提供任意多写副本的最后管理员原子保证。传输在所有节点共享受管版本化密钥环与 registry，不依赖会话粘性；重启加载原 ring，轮换先分发新旧版本再切 activeKeyId。Web 使用 HttpOnly refresh Cookie + 内存 access token；仅同时满足客户端标识、body 通道头与受信原生 Origin 的 App/Desktop 请求返回 refresh token body。

业务修改提交后的审计记录采用最大努力策略：写入失败输出包含操作及资源的结构化错误日志，业务仍返回原操作结果，避免将已经完成的变更误报为失败；应用启动时 audit 初始化失败则停止启动。

### SpaceGate 上游配置

`src/server/upstream/spacegate-upstream-client.ts` 适配既有 Rust `admin-server` 与 AI Gateway 管理 API。`config/_spacegate.yml` 由 `core.config` 加载，敏感值只从服务端环境变量注入，首次上游请求时才校验；`allowInsecure: false` 且缺少 ak/sk 会直接报配置错误，不会静默降级。

| 配置键 | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `adminBaseUrl` | `HAI_SPACEGATE_ADMINBASEURL` | `http://127.0.0.1:9992` | Rust admin-server 基地址，生产应仅内网可达 |
| `adminAk` / `adminSk` | `HAI_SPACEGATE_ADMINAK` / `HAI_SPACEGATE_ADMINSK` | 空 | 服务身份凭据；必须同时配置 |
| `aiGatewayBaseUrl` | `HAI_SPACEGATE_AIGATEWAYBASEURL` | `http://127.0.0.1:18080` | AI Gateway `/v1/admin/...` 基地址 |
| `allowInsecure` | `HAI_SPACEGATE_ALLOWINSECURE` | `false` | 仅无认证的本地上游可设为 `true` |
| `requestTimeoutMs` | `HAI_SPACEGATE_REQUESTTIMEOUTMS` | `10000` | 单次上游请求超时（毫秒） |
| `tokenTtlSeconds` | `HAI_SPACEGATE_TOKENTTLSECONDS` | `3000` | 上游 JWT 内存缓存时长（秒），401 时提前失效 |

上游 JWT 只缓存在服务进程内存；日志只记录 method/path/status，不输出 ak/sk、JWT 或 LDAP 凭据。写操作成功后按 Rust 协议重新读取 `GET /config/names` 获取新版本，409 冲突不会重试。

### IAM 与 LDAP 登录

密码登录默认启用；公开注册默认关闭（`register.enabled=false`）。`register.defaultEnabled=true` 只决定新建（含 LDAP 自动同步）用户默认启用，不等于开放公开注册。`config/_iam.yml` 保留完整注释，LDAP 默认 `login.ldap: false`。服务包显式依赖 catalog 中的 `ldapts@9.2.0`，启用 LDAP 且未注入 `ldapClientFactory` 时由 IAM 加载默认客户端；测试只注入工厂，不访问真实目录。

| 配置键 | 环境变量 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `login.ldap` | `HAI_IAM_LOGIN_LDAP` | `false` | 是否启用 LDAP 登录 |
| `ldap.url` | `HAI_IAM_LDAP_URL` | 示例占位 | LDAP URL；`ldaps://` 或配合 `useTls` 的 `ldap://` |
| `ldap.bindDn` | `HAI_IAM_LDAP_BINDDN` | 示例占位 | 只读检索服务账号 DN |
| `ldap.bindPassword` | `HAI_IAM_LDAP_BINDPASSWORD` | 空 | 绑定密码；仅部署环境/Secret Manager 注入，不写入 YAML 或镜像 |
| `ldap.searchBase` | `HAI_IAM_LDAP_SEARCHBASE` | 示例占位 | 用户搜索 base DN |
| `ldap.searchFilter` | `HAI_IAM_LDAP_SEARCHFILTER` | `(uid={{username}})` | `{{username}}` 由 IAM 转义后替换 |
| `ldap.usernameAttribute` | `HAI_IAM_LDAP_USERNAMEATTRIBUTE` | `uid` | 同步到本地用户名的属性 |
| `ldap.emailAttribute` | `HAI_IAM_LDAP_EMAILATTRIBUTE` | `mail` | 同步到邮箱的属性 |
| `ldap.displayNameAttribute` | `HAI_IAM_LDAP_DISPLAYNAMEATTRIBUTE` | `cn` | 同步到显示名的属性 |
| `ldap.useTls` | `HAI_IAM_LDAP_USETLS` | `false` | `ldap://` 时启用 STARTTLS；`ldaps://` 已使用 TLS |
| `ldap.connectTimeout` | `HAI_IAM_LDAP_CONNECTTIMEOUT` | `5000` | 连接/操作超时（毫秒） |

首次 LDAP 登录按 `rbac.defaultRole` 同步为普通 `user`，不会按用户名或邮件域自动提权；管理员需在现有用户/角色管理中显式授予 `admin`。IAM 的 LDAP 自动同步沿用 `register.defaultEnabled`，默认已为 `true`，保证同步用户可登录；公开注册仍由 `register.enabled=false` 保持关闭。密码登录与 LDAP 登录都写入审计（`action=login`、`resource=auth`），LDAP 记录附带 `{ strategy: 'ldap' }`；审计与日志不包含密码或绑定密码。首个管理员仍由 `HAI_ADMIN_DEFAULT_PASSWORD` 在用户表为空时创建，已有用户时不会重设密码。

### 微信小程序原生通道

微信 `wx.request` 无法实现主通道的 SM2 双向加密。服务端提供第二个 `serv.createApp` 实例，只有设置 `HAI_SERV_MINIAPP_PORT` 时才在 `server.host` 上额外监听：

| 项 | 值 |
| --- | --- |
| 环境变量 | `HAI_SERV_MINIAPP_PORT`（例如 `3001`）；未设置则关闭通道 |
| API 前缀 | `/api/miniapp/v1` |
| contract / procedures / IAM | 与主通道完全相同；SpaceGate 管理路由仍要求 `admin` 角色 |
| 传输加密 | 关闭（仅此通道）；主通道 `/api/v1` 仍强制 SM2 |
| refresh token | 仅 `X-Request-Client: spacegate-admin-miniapp` + `X-HAI-Token-Transport: body` 的请求从 JSON body 读写 |
| 客户端约束 | 所有 `/api/miniapp/v1/*` 业务请求必须携带上述客户端标识，否则 403 |
| OpenAPI/docs | 不暴露 |

安全前置条件：该通道必须置于同一受信 origin 的 HTTPS 反向代理之后，端口只绑定回环/内网，反向代理只接受受信来源；禁止直接暴露公网。不需要小程序时不要设置 `HAI_SERV_MINIAPP_PORT`。主 `serv.listen` 独占 `closeModules`，小程序通道只关闭自身 HTTP 句柄以避免模块重复释放。测试见 `tests/miniapp-channel.test.ts`（login body refresh token、未认证业务路由 401、主/小程序通道加密面差异）。

### 开发

所有配置字段以模块 Schema 为准；未使用字段和替代 Provider 用注释保留。叶子环境变量只覆盖已有 YAML 路径，先取消相应键的注释，或用 `HAI_DB` / `HAI_CACHE` 等整体对象覆盖；`connectTimeout` 对应 `HAI_CACHE_CONNECTTIMEOUT`。凭据示例在根 `.env.example`，实际密钥保存在忽略的 `.env.local` 或部署环境。`.gitignore` 的 `**/data/` 忽略所有运行数据及 `.gitkeep`；运行时会创建目录。`crypto.init()` 无配置参数，datapipe 为纯函数，不提供虚构配置。

CLI 同时在根 `.env.local` 一次性生成独立开发用 `HAI_CRYPTO_TRANSPORT_SERVERKEYRING`，结构为 `{activeKeyId,keys:{id:{publicKey,privateKey}}}`，运行时不隐式生成。部署由 Secret Manager 向所有节点分发相同受管 ring，客户端 registry 复用共享 reldb。私钥不进入普通 YAML、PUBLIC_/VITE_、日志或公开响应。轮换先分发含新旧版本的 ring，再切 activeKeyId 并滚动重建；过渡期后删除旧 ID，旧客户端 destroy 后重协商。没有 sticky session 依赖。

自动生成的 `.env` 包含已启用模块的环境参数；YAML 显式引用相同变量并保留本地回退。使用 `start:test` / `start:prod` 加载对应环境覆盖。

- `pnpm --filter spacegate-admin-serv dev`
- `pnpm --filter spacegate-admin-serv test`

### 容器运行

从工程根运行 `pnpm build --target docker` 导出镜像。默认带部署工具，可运行 `pnpm run deploy --env test --build` 一次完成构建发布，或 `pnpm run deploy --env prod --bundle-only` 交付已有镜像及跨平台脚本。完整包包含自建中间件，人工部署与按服务更新见工程根 README。

- 服务镜像直接由 `@h-ai/serv` 对外监听，配置挂载点为 `/app/config`，数据目录为 `/app/data`，不包含额外转发进程。
- Web 镜像不烘焙 API 地址；启动时使用 `-e PUBLIC_API_BASE=https://api.example.com` 注入，同一镜像可部署到不同环境。未设置时 Web 默认请求自身 origin。
- YAML 配置使用 `${ENV_NAME:default}` 接收运行时环境变量；不要把密钥写入镜像。

### IAM 认证配置

服务端 `config/_iam.yml` 统一控制登录、注册、重置、协议与 `ui` 品牌。邮件/短信可用 `_reach.yml` 中的 Provider/标准模板或自定义发送回调。`AuthPanel` 使用安全公开配置及框架认证动作；无需在页面适配 Token、验证码倒计时和错误提示。单实例可使用 Memory；多实例需共享数据库与 Redis，并按工程根 README 配置写入路由和传输会话。

LDAP/Reach 均可选：基础认证无需 Reach。服务端已安装 `ldapts`，启用 LDAP 后可使用默认客户端；测试通过注入 `ldapClientFactory` 隔离外部目录。自定义 onOtpSendEmail/onOtpSendSms/onPasswordResetRequest 回调可独立于 Reach 使用，投递失败须拒绝 Promise；回调第三参数 context 提供 expiresAt/idempotencyKey，发送服务按稳定键持久化去重，异常/超时为结果未知且不自动重发。协议在 Modal 显示；LoginForm/RegisterForm 提供 header/extraFields/footer，AuthPanel 透传 loginHeader/loginFields/loginFooter 和 registerHeader/registerFields/registerFooter。配置及例子见生成项目的 hai-iam/hai-ui skills。

协议支持配置中的 userAgreementMarkdown/privacyPolicyMarkdown 正文及 URL；同主题优先 Markdown，由安全渲染器在弹窗显示。

完整契约见生成应用的 `.agents/skills/hai-iam/reference.md`。

## 开发测试

```sh
pnpm --filter spacegate-admin-serv typecheck
pnpm --filter spacegate-admin-serv test
```

## 许可证

Apache-2.0
