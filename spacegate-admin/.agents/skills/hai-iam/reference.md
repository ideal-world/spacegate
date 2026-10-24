# hai-iam — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

用户的 `checkPermission(userId, code)` / `getUserAuthorization` 仅查询主体授权，不含请求的 API Key scope；受保护请求使用 `iam.auth.verifyToken` 的已收敛会话权限。API Key 的完整配置、交集示例和撤销流程见对应小节。

角色成员数使用 `getRoleUserCounts(roleIds)`，一次聚合返回 `HaiResult<Map<string, number>>`；成功时无关联为 0，失败必须展示错误，不能伪造零值。

角色资料和权限必须通过 `createRole({ ..., permissionIds })` / `updateRole(id, { ..., permissionIds })` 一次事务保存，禁止在应用层并行增删权限模拟替换。空数组清空、未传保持、未知权限失败、重复 ID 去重；并发以最后提交的完整集合为准。自管事务提交后同步会话，外部事务由调用方提交后同步。

管理界面自动创建角色时使用 `role_${core.id.generate()}` 生成 code，显示名称独立保存；不要通过移除中文、表情或折叠大小写生成标识。重命名不重建 code。

> `@h-ai/iam` 是统一的身份与访问管理模块，支持多种认证策略（密码/OTP/LDAP/API Key）、Bearer Token 管理和 RBAC 授权。依赖 `@h-ai/reldb`（持久化）、`@h-ai/cache`（会话 / OTP / 一次性票据）、`@h-ai/crypto`（密码哈希）、`@h-ai/audit`（审计日志）。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端通过 `@h-ai/api-client` 的 typed client 调用 IAM API，例如 `apiClient.iam.auth.login()`。HTTP 契约统一来自 `@h-ai/api-contract`。

## 依赖

| 模块           | 用途                           | 是否必需 | 初始化要求                 |
| -------------- | ------------------------------ | -------- | -------------------------- |
| `@h-ai/reldb`  | 数据库（用户/角色/权限持久化） | **必需** | 需在 `iam.init()` 前初始化 |
| `@h-ai/cache`  | 缓存（会话/OTP/一次性票据）    | **必需** | 需在 `iam.init()` 前初始化 |
| `@h-ai/crypto` | 密码哈希                       | 内部使用 | 自动初始化                 |
| `@h-ai/audit`  | 审计日志（RBAC 关键操作记录）  | 内部使用 | 已初始化时自动写入         |

## 适用场景

- 用户注册与登录（密码/OTP/LDAP/API Key）
- Bearer Token 认证与管理（TokenPair: accessToken + refreshToken）
- Token 刷新（rotation 策略）与吊销
- RBAC 角色/权限的创建、分配与检查
- API Key 管理（创建、吊销、验证）
- 密码重置流程
- 与 kit 集成的认证守卫
- 通过 `@h-ai/api-contract` 暴露 IAM HTTP contract

## 使用步骤

### 1. 配置

```yaml
# config/_iam.yml
password:
  minLength: 8
  requireUppercase: true
  requireNumber: true
session:
  maxAge: 86400
  refreshTokenMaxAge: 604800 # 7 天
  sliding: true
  singleDevice: false
login:
  password: true
  otp: false
register:
  enabled: true
security:
  maxLoginAttempts: 5
  lockoutDuration: 900
rbac:
  defaultRole: user
  superAdminRole: super_admin
```

约定环境变量优先于 YAML，例如 `HAI_IAM_PASSWORD_MINLENGTH`、
`HAI_IAM_SESSION_MAXAGE`、`HAI_IAM_SECURITY_MAXLOGINATTEMPTS`。

### 2. 初始化

```typescript
import type { IamConfigSettingsInput } from '@h-ai/iam'
import { cache } from '@h-ai/cache'
import { core } from '@h-ai/core'
import { iam } from '@h-ai/iam'
import { reldb } from '@h-ai/reldb'

// 位于应用启动函数内；依赖初始化失败时立即返回，不能继续启动 IAM。
core.init({ configDir: './config' })
const db = await reldb.init({ type: 'sqlite', database: './data.db' })
if (!db.success)
  return db
const cached = await cache.init({ type: 'memory' })
if (!cached.success)
  return cached
const initialized = await iam.init(core.config.getOrThrow<IamConfigSettingsInput>('iam'))
if (!initialized.success)
  return initialized
// 关闭时按 iam → cache → reldb 的反向依赖顺序执行。
```

`iam.config` 返回的是脱敏后的配置快照；LDAP `url` / `bindPassword` 等敏感值会自动隐藏。

基础初始化创建用户/RBAC 五张表；启用 API Key 登录时另创建 `hai_iam_api_keys`。

Token 和 OTP 存储在 cache 中（不落库）。session/refresh/index 位于同一用户分区版本文档，通过单键 CAS 提交。

### 可选 LDAP 与通知

LDAP 默认关闭，Reach 不属于 IAM 的启动前置依赖。`ldapts` 和 `@h-ai/reach` 是可选 peer dependency，基础密码/RBAC 无需安装这两个包。

- 默认 LDAP 客户端：在服务端 workspace 执行 `pnpm add ldapts`，配置 `login.ldap: true` 与 `ldap.url/bindDn/bindPassword/searchBase`。`searchFilter` 默认 `(uid={{username}})`；AD 可使用 `(sAMAccountName={{username}})` 与对应 `usernameAttribute`。`ldaps://` 使用 TLS，`ldap://` + `useTls: true` 使用 StartTLS。`ldapClientFactory` 可注入返回 `bind/search/unbind` 和 HaiResult 的客户端，此时无需安装 ldapts。
- Reach 自动发送：应用安装、初始化 `@h-ai/reach`，在 `iam.init({ ...settings, reach })` 中注入一次。`otp_email/otp_sms` 模板使用 `{code}`；`password_reset/password_reset_sms` 使用 `{token}/{expiresAt}/{resetUrl}`。模板需在 IAM 初始化前配置，Provider 绑定实际邮件/短信通道。`passwordReset.mode` 选择 `email/phone`。
- 自定义发送：直接传 `onOtpSendEmail(email, code, context)`、`onOtpSendSms(phone, code, context)`、`onPasswordResetRequest(user, token, context)`，均返回 `Promise<void>`；context 包含 expiresAt/idempotencyKey，发送服务按该稳定键持久化去重，异常表示结果未知，不自动重发。可以只实现选用通道或与 Reach 混用，显式回调优先。检查发送服务结果，投递失败在回调内抛错；IAM 捕获并返回业务错误，不要吞掉发送失败。
- `otp` 必须有配置并开启所需通道；缺少回调或标准模板时 `iam.publicConfig` 不公布该入口。Reach 由应用管理生命周期，IAM 不负责初始化或关闭。Console 仅记录元数据。

协议以 Modal 显示，关闭后保留输入。URL 方式必须为 HTTP(S) 且页面允许 iframe 嵌入；脚本被隔离禁用。`LoginForm/RegisterForm` 提供 `header/extraFields/footer`；`AuthPanel` 提供 `loginHeader/loginFields/loginFooter`、`registerHeader/registerFields/registerFooter`。额外字段由应用绑定状态，在自己的动作中处理，IAM 不接收未经定义的字段。

密码登录只接收 identifier/password；AuthPanel 将 LoginForm 的 username 映射为 identifier。会话期限来自服务端 session.maxAge/refreshTokenMaxAge，不增加未实现的登录偏好字段或持久会话选择框。

## 核心 API

### 认证 — `iam.auth`

| 方法               | 签名                                                           | 说明                                             |
| ------------------ | -------------------------------------------------------------- | ------------------------------------------------ |
| `login`            | `({ identifier, password }) => Promise<HaiResult<AuthResult>>` | 密码登录（用户名/邮箱/手机号）                   |
| `loginWithOtp`     | `({ identifier, code }) => Promise<HaiResult<AuthResult>>`     | OTP 验证码登录                                   |
| `loginWithLdap`    | `({ username, password }) => Promise<HaiResult<AuthResult>>`   | LDAP 登录                                        |
| `loginWithApiKey`  | `({ key }) => Promise<HaiResult<AuthResult>>`                  | API Key 登录                                     |
| `sendOtp`          | `(identifier) => Promise<HaiResult<{ expiresAt }>>`            | 发送 OTP 验证码                                  |
| `verifyToken`      | `(token) => Promise<HaiResult<Session>>`                       | 验证会话并读取当前用户状态及授权；数据库失败拒绝 |
| `logout`           | `(token) => Promise<HaiResult<void>>`                          | 登出                                             |
| `registerAndLogin` | `(options: RegisterOptions) => Promise<HaiResult<AuthResult>>` | 注册并登录（一站式）                             |

**AuthResult**（已改造为 TokenPair）：

```typescript
interface AuthResult {
  user: User
  tokens: TokenPair
  roles: string[]
  permissions: string[]
  agreements?: AgreementDisplay
}

interface TokenPair {
  accessToken: string
  refreshToken: string
  expiresIn: number
  tokenType: 'Bearer'
}
```

### 会话管理 — `iam.session`

| 方法                | 签名                                                                    | 说明                                     |
| ------------------- | ----------------------------------------------------------------------- | ---------------------------------------- |
| `create`            | `({ userId, username, roles, source? }) => Promise<HaiResult<Session>>` | 创建会话（返回含 TokenPair 的 session）  |
| `get`               | `(token) => Promise<HaiResult<Session \| null>>`                        | 获取会话（滑动续期自动延长）             |
| `verifyToken`       | `(token) => Promise<HaiResult<Session>>`                                | 验证令牌                                 |
| `update`            | `(token, data, expectedVersion) => Promise<HaiResult<void>>`            | 更新会话字段（浅合并 data）              |
| `delete`            | `(token) => Promise<HaiResult<void>>`                                   | 删除会话                                 |
| `deleteByUserId`    | `(userId) => Promise<HaiResult<number>>`                                | 删除用户所有会话（返回删除数量）         |
| `refresh`           | `(refreshToken) => Promise<HaiResult<TokenPair>>`                       | 刷新 Token（rotation：旧 Token 失效）    |
| `revokeRefresh`     | `(refreshToken) => Promise<HaiResult<void>>`                            | 吊销 refreshToken                        |
| `patchUserSessions` | `(userId, updates: SessionFieldUpdates) => Promise<HaiResult<void>>`    | 批量更新用户所有会话的 roles/permissions |

**Token 刷新（rotation 策略）**：

- `refresh()` 会删除旧的 accessToken + refreshToken
- 创建全新的 session，返回新的 TokenPair
- 旧 refreshToken 立即失效，防止重放攻击

### 一次性票据 — `iam.ticket`

短期、一次性能力票据，用于无法携带 Bearer Token 的通道（如 WebSocket URL）。密码学安全随机、TTL、原子单次消费、用途绑定。

| 方法      | 签名                                                                                    | 说明                               |
| --------- | --------------------------------------------------------------------------------------- | ---------------------------------- |
| `issue`   | `({ subjectId, purpose, grant?, ttlMs? }) => Promise<HaiResult<{ ticket, expiresAt }>>` | 签发（默认 30s）                   |
| `consume` | `(ticket, { purpose? }?) => Promise<HaiResult<{ subjectId, purpose, grant }>>`          | 原子消费（单次有效，用途不符拒绝） |

```ts
const issued = await iam.ticket.issue({ subjectId, purpose: 'ai-audio', grant: { operation: 'transcribe' } })
const consumed = await iam.ticket.consume(ticket, { purpose: 'ai-audio' }) // 并发消费仅一个成功
```

错误码：`TICKET_INVALID`(hai:iam:110) / `TICKET_EXPIRED`(hai:iam:111) / `TICKET_ISSUE_FAILED`(hai:iam:112)。

### 用户管理 — `iam.user`

| 方法                        | 签名                                                                        | 说明                                                              |
| --------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `register`                  | `(input) => Promise<HaiResult<RegisterResult>>`                             | 用户注册                                                          |
| `getUser`                   | `(id, options?) => Promise<HaiResult<User \| null>>`                        | 获取用户（可选 include: ['roles']）                               |
| `getCurrentUser`            | `(token) => Promise<HaiResult<User>>`                                       | 通过令牌获取当前用户                                              |
| `updateCurrentUser`         | `(token, data: UpdateCurrentUserInput) => Promise<HaiResult<User>>`         | 当前用户修改个人资料（白名单字段）                                |
| `getUsers`                  | `(ids, options?) => Promise<HaiResult<User[]>>`                             | 批量公开用户，上限 1000；去重输入顺序、缺失省略、可 include roles |
| `listUsers`                 | `(options?: ListUsersOptions) => Promise<HaiResult<PaginatedResult<User>>>` | 用户列表（分页 + 搜索 + roleCode 分页前筛选 + 白名单排序）        |
| `updateUser`                | `(id, data) => Promise<HaiResult<User>>`                                    | 更新用户信息                                                      |
| `deleteUser`                | `(id) => Promise<HaiResult<void>>`                                          | 删除用户（自动清除会话）                                          |
| `changePassword`            | `(id, old, new) => Promise<HaiResult<void>>`                                | 修改密码（自动清除会话）                                          |
| `changeCurrentUserPassword` | `(token, old, new) => Promise<HaiResult<void>>`                             | 当前用户修改密码（通过令牌识别）                                  |
| `requestPasswordReset`      | `(email) => Promise<HaiResult<void>>`                                       | 请求密码重置（防枚举）                                            |
| `confirmPasswordReset`      | `(token, newPassword) => Promise<HaiResult<void>>`                          | 确认密码重置（自动清除会话）                                      |
| `adminResetPassword`        | `(userId, newPassword) => Promise<HaiResult<void>>`                         | 管理员直接重置密码                                                |
| `validatePassword`          | `(password) => HaiResult<void>`                                             | 验证密码强度（**同步方法**）                                      |

### RBAC 授权 — `iam.authz`

| 方法                        | 签名                                                                                    | 说明                                                         |
| --------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `checkPermission`           | `(userId, permission) => Promise<HaiResult<boolean>>`                                   | 检查权限（超管自动通过，支持通配符）                         |
| `getUserAuthorization`      | `(userId) => Promise<HaiResult<{roles: string[], permissions: string[]}>>`              | 当前角色及权限代码，角色 JOIN + 批量权限查询；失败不回退缓存 |
| `getUserPermissions`        | `(userId) => Promise<HaiResult<Permission[]>>`                                          | 获取用户权限列表（通过角色聚合去重）                         |
| `getUserRoles`              | `(userId) => Promise<HaiResult<Role[]>>`                                                | 获取用户角色列表                                             |
| `getUserRolesForMany`       | `(userIds) => Promise<HaiResult<Map<string, Role[]>>>`                                  | 批量获取多用户角色（避免 N+1）                               |
| `assignRole`                | `(userId, roleId, tx?) => Promise<HaiResult<void>>`                                     | 分配角色给用户                                               |
| `removeRole`                | `(userId, roleId, tx?) => Promise<HaiResult<void>>`                                     | 移除用户角色                                                 |
| `syncRoles`                 | `(userId, roleIds, tx?) => Promise<HaiResult<void>>`                                    | 同步用户角色（替换为目标列表）                               |
| `createRole`                | `({ code, name, description? }, tx?) => Promise<HaiResult<Role>>`                       | 创建角色                                                     |
| `getRole`                   | `(roleId) => Promise<HaiResult<Role \| null>>`                                          | 获取角色                                                     |
| `getRoleByCode`             | `(code) => Promise<HaiResult<Role \| null>>`                                            | 根据代码获取角色                                             |
| `getAllRoles`               | `(options?) => Promise<HaiResult<PaginatedResult<Role>>>`                               | 获取所有角色（分页）                                         |
| `updateRole`                | `(roleId, data, tx?) => Promise<HaiResult<Role>>`                                       | 更新角色                                                     |
| `deleteRole`                | `(roleId, tx?) => Promise<HaiResult<void>>`                                             | 删除角色（级联清理关联 + 会话同步）                          |
| `createPermission`          | `({ code, name, type?, resource?, action? }, tx?) => Promise<HaiResult<Permission>>`    | 创建权限                                                     |
| `getPermission`             | `(permissionId) => Promise<HaiResult<Permission \| null>>`                              | 获取权限                                                     |
| `getPermissionByCode`       | `(code) => Promise<HaiResult<Permission \| null>>`                                      | 根据代码获取权限                                             |
| `getAllPermissions`         | `(options?: PermissionQueryOptions) => Promise<HaiResult<PaginatedResult<Permission>>>` | 获取所有权限（分页 + 类型/关键字筛选）                       |
| `deletePermission`          | `(permissionId, tx?) => Promise<HaiResult<void>>`                                       | 删除权限（级联清理关联 + 会话同步）                          |
| `assignPermissionToRole`    | `(roleId, permId, tx?) => Promise<HaiResult<void>>`                                     | 分配权限给角色                                               |
| `removePermissionFromRole`  | `(roleId, permId, tx?) => Promise<HaiResult<void>>`                                     | 移除角色权限                                                 |
| `getRolePermissions`        | `(roleId) => Promise<HaiResult<Permission[]>>`                                          | 获取角色的权限列表                                           |
| `getRolePermissionsForMany` | `(roleIds) => Promise<HaiResult<Map<string, Permission[]>>>`                            | 批量获取多角色权限（避免 N+1）                               |

**通配符规则**：`admin:*` 匹配 `admin:read`、`admin:write` 等。超管角色自动拥有所有权限。

### API Key 管理 — `iam.apiKey`

先在初始化配置中启用 API Key。它是用户的受限凭据，不能独立授予用户未拥有的权限：**有效权限 = 当前主体权限 ∩ scopes**。scopes 是与权限 code 相同的字符串，不是角色名、菜单名称或资源 ID；不会创建权限，也不会替代租户/记录归属校验。

```yaml
# config/_iam.yml
login:
  apikey: true
apikey:
  maxKeysPerUser: 10
  defaultExpirationDays: 30
```

假设主体拥有 `record:read`、`record:write`、`user:read`，不同 scopes 的结果如下（主体授权改变后重新计算）：

| scopes                         | 有效权限                      | roles        |
| ------------------------------ | ----------------------------- | ------------ |
| 未传或 `[]`                    | 无操作权限，仅可识别身份      | `[]`         |
| `['record:read']`              | `record:read`                 | `[]`         |
| `['record:*']`                 | `record:read`、`record:write` | `[]`         |
| `['record:read', 'user:read']` | 两个指定权限                  | `[]`         |
| `['missing:read']`             | 主体未拥有，因此无权限        | `[]`         |
| `['*']`                        | 主体当前拥有的全部权限        | 保留主体角色 |

`record:*` 匹配冒号后的全部层级，不支持任意正则或 `record:r*` 等中间通配符。主体仅拥有 `record:read` 时，`['record:*']` 仍只得到读取权限；主体持有通配符时，交集保留较窄范围。受限 Key 没有角色入口，使用 permission 守卫；显式 `*` 是宽授权，按实际需要选择。

以下为创建 → Key 登录 → Bearer 会话验证与操作授权 → 列表 → 撤销的服务端示例。先检查所有 HaiResult；每个管理入口还要校验本人归属或管理员权限，IAM 不知道业务的调用者身份。

```ts
import { err, HaiCommonError, ok } from '@h-ai/core'
import { iam } from '@h-ai/iam'

async function createReadKey(userId: string) {
  // 调用前校验管理权限或本人归属；scopes 使用权限 code。
  const created = await iam.apiKey.createApiKey(userId, {
    name: 'Read client',
    expirationDays: 30,
    scopes: ['record:read'],
  })
  if (!created.success)
    return created
  // rawKey 仅本次返回；通过安全渠道展示一次，不写日志。
  return created
}

async function apiKeyLogin(rawKey: string) {
  // 通常由客户端取得 Key 后调用；服务器内部验证并绑定 Key。
  return iam.auth.loginWithApiKey({ key: rawKey })
}

async function requireRecordRead(accessToken: string, deniedMessage: string) {
  // deniedMessage 由应用 i18n 提供；先读取当前且已收敛 scope 的会话。
  const verified = await iam.auth.verifyToken(accessToken)
  if (!verified.success)
    return verified
  const allowed = verified.data.permissions.some(code =>
    code === '*' || code === 'record:read'
    || (code.endsWith(':*') && 'record:read'.startsWith(code.slice(0, -1))),
  )
  if (!allowed)
    return err(HaiCommonError.FORBIDDEN, deniedMessage)
  // 这里只确认操作权限；业务层还须检查具体记录的数据归属。
  return ok({ userId: verified.data.userId })
}

async function listMyKeys(userId: string) {
  return iam.apiKey.listApiKeys(userId, {
    page: 1,
    pageSize: 20,
    search: 'Read',
    enabled: true,
  })
}

async function revokeKey(keyId: string) {
  // 调用前校验本人归属或管理权限。
  return iam.apiKey.revokeApiKey(keyId)
}
```

创建后只有 `rawKey` 能用于 Key 登录；`apiKey.id` 是管理 ID，`keyPrefix` 仅用于显示/检索，均不能代替 rawKey。`verifyApiKey(rawKey)` 仅验证 Key 并返回元数据，不能代替用户状态、scopes 与操作授权。

Key 登录会话绑定 Key ID。刷新使用普通 `iam.session.refresh`，不会丢失该绑定或扩大 scopes。撤销成功后，已有 accessToken 的 get/verify 以及 refreshToken 的 refresh 都会失败；过期、禁用及持久化读取失败同样拒绝。角色撤权由 `iam.auth.verifyToken` 读取当前授权生效。改 scope 使用创建新 Key 后撤销旧 Key，当前 API 不提供 scope 更新操作。

`expirationDays: 0` 表示不设过期时间，省略时使用 `apikey.defaultExpirationDays`；需要限时访问时显式设置。明文 Key 和 Token 不进入 URL、日志、公开配置或普通 localStorage。

| 方法           | 签名                                                                               | 说明                                 |
| -------------- | ---------------------------------------------------------------------------------- | ------------------------------------ |
| `createApiKey` | `(userId, options: CreateApiKeyOptions) => Promise<HaiResult<CreateApiKeyResult>>` | 创建 API Key（明文密钥仅返回一次）   |
| `listApiKeys`  | `(userId, options?) => Promise<HaiResult<PaginatedResult<ApiKey>>>`                | 分页列出用户的 API Key               |
| `getApiKey`    | `(keyId) => Promise<HaiResult<ApiKey \| null>>`                                    | 获取 API Key 详情                    |
| `revokeApiKey` | `(keyId) => Promise<HaiResult<void>>`                                              | 吊销/删除 API Key                    |
| `verifyApiKey` | `(rawKey) => Promise<HaiResult<ApiKey>>`                                           | 验证 API Key 并返回实体（含用户 ID） |

## HTTP API 契约

IAM HTTP API 统一由 `@h-ai/api-contract` 的 `apiContract.iam` 定义，由 `@h-ai/serv/features/iam` 绑定到本模块。

```typescript
import { apiClient } from '@h-ai/api-client'

const result = await apiClient.iam.auth.login({ identifier: username, password })
```

## 错误码 — `HaiIamError`

| 错误码                                  | code          | 说明             |
| --------------------------------------- | ------------- | ---------------- |
| `HaiIamError.AUTH_FAILED`               | `hai:iam:001` | 认证失败         |
| `HaiIamError.INVALID_CREDENTIALS`       | `hai:iam:002` | 凭证无效         |
| `HaiIamError.USER_NOT_FOUND`            | `hai:iam:003` | 用户不存在       |
| `HaiIamError.USER_DISABLED`             | `hai:iam:004` | 用户已禁用       |
| `HaiIamError.USER_LOCKED`               | `hai:iam:005` | 用户已锁定       |
| `HaiIamError.USER_ALREADY_EXISTS`       | `hai:iam:006` | 用户已存在       |
| `HaiIamError.PASSWORD_EXPIRED`          | `hai:iam:007` | 密码已过期       |
| `HaiIamError.PASSWORD_POLICY_VIOLATION` | `hai:iam:008` | 密码不符合策略   |
| `HaiIamError.OTP_INVALID`               | `hai:iam:009` | 验证码无效       |
| `HaiIamError.NOT_INITIALIZED`           | `hai:iam:010` | 未初始化         |
| `HaiIamError.OTP_RESEND_TOO_FAST`       | `hai:iam:011` | 发送过于频繁     |
| `HaiIamError.LOGIN_DISABLED`            | `hai:iam:012` | 登录方式已禁用   |
| `HaiIamError.REGISTER_DISABLED`         | `hai:iam:013` | 注册已禁用       |
| `HaiIamError.STRATEGY_NOT_SUPPORTED`    | `hai:iam:014` | 认证策略不支持   |
| `HaiIamError.APIKEY_INVALID`            | `hai:iam:015` | API Key 无效     |
| `HaiIamError.APIKEY_EXPIRED`            | `hai:iam:016` | API Key 已过期   |
| `HaiIamError.APIKEY_DISABLED`           | `hai:iam:017` | API Key 已禁用   |
| `HaiIamError.APIKEY_NOT_FOUND`          | `hai:iam:018` | API Key 不存在   |
| `HaiIamError.RESET_TOKEN_INVALID`       | `hai:iam:019` | 重置令牌无效     |
| `HaiIamError.RESET_TOKEN_EXPIRED`       | `hai:iam:020` | 重置令牌已过期   |
| `HaiIamError.RESET_TOKEN_MAX_ATTEMPTS`  | `hai:iam:021` | 重置验证次数超限 |
| `HaiIamError.OTP_EXPIRED`               | `hai:iam:022` | 验证码已过期     |
| `HaiIamError.SESSION_NOT_FOUND`         | `hai:iam:101` | 会话不存在       |
| `HaiIamError.SESSION_EXPIRED`           | `hai:iam:102` | 会话已过期       |
| `HaiIamError.SESSION_INVALID`           | `hai:iam:103` | 会话无效         |
| `HaiIamError.SESSION_CREATE_FAILED`     | `hai:iam:104` | 会话创建失败     |
| `HaiIamError.TOKEN_EXPIRED`             | `hai:iam:105` | 令牌已过期       |
| `HaiIamError.TOKEN_INVALID`             | `hai:iam:106` | 令牌无效         |
| `HaiIamError.TOKEN_REFRESH_FAILED`      | `hai:iam:107` | 令牌刷新失败     |
| `HaiIamError.PERMISSION_DENIED`         | `hai:iam:201` | 权限不足         |
| `HaiIamError.ROLE_NOT_FOUND`            | `hai:iam:202` | 角色不存在       |
| `HaiIamError.PERMISSION_NOT_FOUND`      | `hai:iam:203` | 权限不存在       |
| `HaiIamError.ROLE_ALREADY_EXISTS`       | `hai:iam:204` | 角色已存在       |
| `HaiIamError.PERMISSION_ALREADY_EXISTS` | `hai:iam:205` | 权限已存在       |
| `HaiIamError.LDAP_CONNECTION_FAILED`    | `hai:iam:301` | LDAP 连接失败    |
| `HaiIamError.LDAP_BIND_FAILED`          | `hai:iam:302` | LDAP 绑定失败    |
| `HaiIamError.LDAP_SEARCH_FAILED`        | `hai:iam:303` | LDAP 搜索失败    |
| `HaiIamError.REPOSITORY_ERROR`          | `hai:iam:401` | 存储层操作错误   |
| `HaiIamError.NOT_FOUND`                 | `hai:iam:402` | 资源不存在       |
| `HaiIamError.CONFLICT`                  | `hai:iam:403` | 资源冲突         |
| `HaiIamError.FORBIDDEN`                 | `hai:iam:501` | 禁止访问         |
| `HaiIamError.INVALID_ARGUMENT`          | `hai:iam:502` | 参数无效         |
| `HaiIamError.CONFIG_ERROR`              | `hai:iam:901` | 配置错误         |
| `HaiIamError.INTERNAL_ERROR`            | `hai:iam:999` | 内部错误         |

## 常见模式

### 与 kit 集成（hooks.server.ts）

```typescript
import { iam } from '@h-ai/iam'
import { kit } from '@h-ai/kit'

// 应用启动时已成功初始化 IAM。
export const handle = kit.createHandle({
  auth: {
    verifyToken: async (token) => {
      const result = await iam.auth.verifyToken(token)
      return result.success ? result.data : null
    },
    operations: () => iam.auth,
    loginUrl: '/auth/login',
    protectedPaths: ['/admin/*', '/api/*'],
    publicPaths: ['/api/auth/*', '/api/public/*'],
  },
})
```

### 客户端认证流程

```typescript
// 登录 → 保存 Token
const login = await apiClient.iam.auth.login({ identifier: username, password })
if (login.success) {
  await apiClient.auth.setTokens(login.data.tokens)
}

// 获取当前用户
const me = await apiClient.iam.auth.currentUser()
if (me.success) {
  // 返回 User + 服务端当前会话的 roles/permissions。
  // 身份资料与访问范围可在状态层拆开，但不要持久化权限快照。
  const { roles, permissions, ...user } = me.data
}

// Token 过期时，api-client 自动调用 refreshPath 刷新
// 刷新失败时，清除 Token 并跳转登录页

// 登出：本地清理与服务端撤销确认分别处理，调用方继续检查返回结果。
async function signOut() {
  const result = await apiClient.iam.auth.logout({})
  await apiClient.auth.clear()
  return result
}
```

### 自动会话失效

以下操作自动清除受影响用户的所有活跃会话：

- `deleteUser` / `updateUser({ enabled: false })` / `changePassword` / `confirmPasswordReset`
- `assignRole` / `removeRole` / `deleteRole` 会同步更新活跃会话中的角色列表

## 配置驱动认证页面

### 配置 Markdown 协议

协议支持 `agreements.userAgreementMarkdown` 与 `privacyPolicyMarkdown` 直接从配置加载；YAML 的 `|` 保留 Markdown 段落和换行。弹窗复用安全 MarkdownRenderer，关闭原始 HTML、脚本和代码复制工具。同主题同时提供正文与 URL 时优先正文，URL 可作为额外查看入口；只有 URL 时使用隔离 iframe。

```yaml
agreements:
  showOnRegister: true
  showOnLogin: false
  userAgreementMarkdown: |
    # 用户协议

    **账户使用条款**

    - 请妥善保管账户凭据。
  privacyPolicyMarkdown: |
    # 隐私政策

    注册资料用于账户管理，会话信息用于身份认证。
  # 可选 URL；同主题同时配置 Markdown 和 URL 时，弹窗优先显示 Markdown。
  # userAgreementUrl: https://example.com/terms
  # privacyPolicyUrl: https://example.com/privacy
```

服务端通过 `iam.publicConfig` 提供白名单配置，或通过客户端的 `client.iam.auth.publicConfig()` 标准 HTTP contract 获取。只展示已开启且具备依赖的登录方式；LDAP 为可选集成，见上方配置与安装说明。

| 配置                                                                                 | 用途                                                  |
| ------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| `login.password / otp / ldap`                                                        | 独立启用密码、验证码、企业账号                        |
| `otp.email / sms`                                                                    | 独立启用邮件、短信                                    |
| `register.enabled / defaultEnabled`                                                  | 自助注册及新账户状态                                  |
| `passwordReset.enabled / mode / resetUrl`                                            | 启用重置、选择 email/phone 渠道、配置重置页面绝对地址 |
| `agreements.showOnRegister / showOnLogin`                                            | 注册/登录协议弹窗显示；链接只接受 HTTP(S)             |
| `ui.variant / title / subtitle / brandTitle / brandText / logoUrl / illustrationUrl` | 卡片/分栏布局、标题、品牌、Logo 和配图                |

选择 Reach 时，在 reldb/cache/Reach 初始化成功后使用 `iam.init({ ...settings, reach })` 自动接入 Reach 的 `otp_email`、`otp_sms`、`password_reset`、`password_reset_sms` 模板。模板绑定对应邮件/短信 Provider；显式发送回调可覆盖默认行为。Console Provider 只记录元数据，实际收信需配置 SMTP、短信或 HTTP 通知服务。发送失败返回错误，不会伪报已发送。

SvelteKit 使用 `kit.auth.createApi(iam)` 导出标准 handler，匿名端点白名单使用 `kit.auth.publicPaths`。客户端通过 `kit.client.createAuthActions()` 连接 Cookie 端点；独立 HTTP 客户端通过 `apiClient.createAuthActions(client)` 连接 IAM contract 并自动保存 Token 到已配置安全存储。将配置及动作直接传入 `AuthPanel`，完成登录、注册、找回及重置；单独使用 `LoginForm` 可传 `methods`、`onsendotp`、`resendInterval`，密码表单可传完整 `passwordPolicy`。

管理员使用 `iam.user.createUser()` 建用户，调用端必须校验管理员权限；关闭自助注册不影响管理端。注册与密码登录开关独立，`registerAndLogin` 在注册完成后创建会话；默认禁用账户不会创建会话。

### 分布式安全与边界

所有节点共享同一数据库和 Redis；Memory 仅用于单进程测试。OTP 的验证码、尝试次数和冷却期保存在同一身份 key，通过 CAS 发布和消费；冲突拒绝旧代，消费/耗尽保留冷却期。密码重置的摘要、有效期、尝试次数和 resetVersion 保存在用户数据库行，策略失败计次，最终一条条件 SQL 同时改密、消费令牌并推进 credentialVersion；新令牌、后续尝试、停用或到期都会阻止迟到提交。发送重置请求只推进 resetVersion，不撤销现有会话；成功改密才推进 credentialVersion。认证策略保存已验证的凭据代际，建立会话/认证/刷新均校验数据库当前代际；缓存批量清理失败也不能使旧会话有效。通知只在成功发布后调用，第三参数为 { expiresAt, idempotencyKey }，应用及 Reach Provider 必须用该稳定键持久化去重。异常/超时可能已送达，返回结果未知，不自动重发或回滚已发布凭据；没有接收端幂等保障时不能承诺外部副作用恰好一次。

可选字段（如 `ldap`）若尚未写入 YAML，请先声明该字段，或通过 `HAI_IAM` 的 JSON 值整体覆盖模块配置；已有字段支持 `HAI_IAM_LOGIN_OTP` 等环境变量覆盖。

标准 HTTP 入口：`GET /auth/config`、`POST /auth/login/ldap`、`POST /auth/password-reset/request`、`POST /auth/password-reset/confirm`；验证码继续使用 `/auth/otp/send` 与 `/auth/login/otp`。安全错误：`SECURITY_BUSY (hai:iam:023)`、`PASSWORD_RESET_DISABLED (hai:iam:024)`。

## 相关 Skills

- `hai-build`：模块初始化顺序（db → cache → iam）
- `hai-kit`：SvelteKit 集成（Bearer Token + 契约处理）
- `hai-api-client`：客户端契约调用
- `hai-reldb`：底层数据存储
- `hai-cache`：会话、OTP 状态和一次性票据
- `hai-crypto`：密码哈希（内部依赖）
- `hai-ui`：IAM 场景组件（LoginForm/RegisterForm 等）

会话与 refresh 消费记录统一保存在 `hai:iam:sessions:{SHA256(userId)}` 版本化文档，不再使用独立 token 映射/索引。Bearer token 携带不可作为凭据的分区提示和 256 位随机值；客户端必须视为不透明字符串。`Session.version` 用于轮换提交校验。Memory 仅用于单进程测试，生产所有节点共享 Redis（含 Cluster）。并发冲突从最新文档重试，耗尽返回 CONFLICT；旧 refresh 不会产生第二个后继。若响应丢失后结果不明，调用方按失败关闭并重新登录，不假定旧凭据仍有效；全部会话仍受同一用户状态索引管理。

`iam.session.update(token, patch, expectedVersion)` 必须传入读取到的 Session.version。旧版本返回 CONFLICT，已撤销 token 返回 SESSION_NOT_FOUND；不会将旧内容重新写回或覆盖新会话。仅滑动时间戳更新不改变内容版本，允许正常并行请求。批量权限更新绑定首次读取的 token/版本集合，不将迟到数据应用到后续登录；遇到较新内容版本失败，调用方重新查询授权后再提交。

`create({ maxAge })` 可覆盖配置默认时长（正安全整数秒），该值持久化为 `Session.maxAge`；签发和刷新返回的 `expiresIn` 均使用它。滑动续期按该时长延长 access，返回已成功提交的 `lastActiveAt/expiresAt`；迟到请求只延长时间，不缩短较新续期。显式 `update` 的有效日期补丁完整生效并产生新内容版本。客户端根据本次返回的 `expiresIn` 安排刷新，不能硬编码配置默认值。

access 与 refresh 独立到期：用户分区保留至所有 access/refresh 的最晚有效期，但 access 认证只检查自己的 `expiresAt`。access 过期仍可凭未过期、未撤销的 refresh 轮换；每次轮换从新签发时刻重新计算 `refreshTokenMaxAge`，旧 access/refresh 同时失效。删除 access、logoutAll、单设备替换均撤销待刷新记录；仅 revokeRefresh 保留仍有效的 access。刷新路由只校验 refresh，不能先要求旧 access 认证成功。

会话仓库将缓存读取/提交失败返回为失败 HaiResult，不解释为会话不存在或过期，也不执行清理补偿。用户会话及索引共用一个文档，批量授权更新不再逐 token 调用 get/ttl 或维护第二份索引；只按已确认的绝对期限清理。故障恢复后可重新读取并重试更新/退出，logoutAll 仍覆盖原有全部记录。结构损坏同样失败关闭并保留原值，交由运维处理，禁止用空文档覆盖未知授权状态。

LDAP 的 ldapSyncUser: false 要求已有启用的本地用户以绑定数据库 credentialVersion；没有本地记录返回 USER_NOT_FOUND，不再创建虚拟 DN 身份。

Session.credentialVersion 绑定创建时的数据库凭据代际；CreateSessionOptions.expectedCredentialVersion 仅供已验证策略传入，旧代拒绝。省略它的受信低层 create 仍读取真实、启用的本地用户并绑定当前代，不能伪造缺少数据库记录的身份。

`iam.auth.logout(accessToken)` 直接按完整不透明令牌在用户分区执行一次 CAS，原子撤销 access 与关联 refresh；access 已到期也不能跳过撤销。非法/不存在的令牌幂等成功，缓存读取或提交故障返回失败，恢复后可用同一令牌重试。HTTP 登出只允许撤销所持令牌，不以 userId 定位他人会话，也不要求该 access 仍通过认证。调用方必须检查 `success`；本地 Cookie/安全存储清理不代表服务端撤销已经确认，失败应显示提示。
