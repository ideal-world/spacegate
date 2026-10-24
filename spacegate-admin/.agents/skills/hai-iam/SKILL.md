---
name: hai-iam
description: "使用 @h-ai/iam 实现登录、注册、会话、OTP/LDAP/API Key 和 RBAC。"
---

# hai-iam

## 能力契约

| 项目     | 契约                                                                                                                                                                    |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 能力     | 使用 @h-ai/iam 实现登录、注册、会话、OTP/LDAP/API Key 和 RBAC                                                                                                           |
| 适用场景 | 应用身份认证、会话失效或角色权限管理                                                                                                                                    |
| 输入     | IamConfigInput、已初始化依赖、凭据、用户/角色/权限 ID                                                                                                                   |
| 输出     | 认证、会话与授权操作的 HaiResult                                                                                                                                        |
| 限制     | 仅服务端，先初始化 reldb/cache（crypto 未初始化时由 IAM 初始化）；公开认证入口是 iam.auth。客户端经 contract/API 调用，凭证只用于认证和安全存储，不进入日志或公开配置。 |

## 使用路径

- 密码登录只接收 identifier/password；AuthPanel 将 LoginForm 的 username 映射为 identifier。会话期限来自服务端 session.maxAge/refreshTokenMaxAge，不增加未实现的登录偏好字段或持久会话选择框。

- `_iam.yml` 只存 IamConfigSettingsInput，运行时回调单独传给 iam.init；不把 reldb/cache 句柄混入配置。
- 角色资料与 permissionIds 一次保存：未传保持、空数组清空、未知 ID 失败；人数查询失败不能显示为 0。
- IAM 服务端是 iam.auth；typed HTTP client 的路径是 apiClient.iam.auth。
- OTP/API Key 等先核对 login 开关；不把未启用能力的 NOT_INITIALIZED 当认证失败。
- API Key 的配置、scope 交集示例表与创建→登录→操作授权→撤销代码见参考中的 API Key 小节。不要以 `checkPermission(userId)` 绕过请求 scope；ID/code/name 分别用于关联、守卫与展示。
- API Key scopes 与主体权限取交集，默认空数组无操作权限；受限 key 无角色授权，使用 permission 守卫。显式 * 仍受主体限制，派生会话读取/刷新检查 key，撤销/禁用/到期立即拒绝。

- 认证页面使用 iam.publicConfig + AuthPanel，协议用弹窗，表单通过 header/extraFields/footer 或 AuthPanel 的 login*/register* snippet 扩展；Kit 使用 kit.auth.createApi/publicPaths + kit.client.createAuthActions，HTTP 客户端使用 apiClient.createAuthActions。
- LDAP/Reach 均可选：基础认证无需安装；默认 LDAP 关闭，选用默认客户端才安装 ldapts 并设置 login.ldap + ldap。OTP 邮件/短信由 otp.email/sms 控制，选择 Reach 标准模板或自定义发送回调；具体签名与失败处理见参考中的“可选 LDAP 与通知”。
- 多节点共享 DB + Redis，Memory 只用于单进程。重置的策略失败计入 maxAttempts；OTP 消费不解除 resendInterval。管理员 createUser 不受自助注册开关限制。

- 受保护请求使用 `iam.auth.verifyToken`：先核验会话绑定的持久凭据代，再复核当前用户启用状态与凭据代并读取当前角色/权限；任一持久读取失败即拒绝。`iam.session.get/verifyToken` 也读取持久用户并校验启用状态与 `credentialVersion`，但返回的角色/权限仍是会话快照，不能替代当前授权。serv 内置 IAM 注入默认使用当前授权入口，无需手动补查。
- 用户批量读取用 `iam.user.getUsers(ids, { include: ['roles'] })`（最多 1000，去重保留输入顺序，缺失省略）；角色筛选用 `listUsers({ roleCode })`，在分页前执行，避免 N+1 和分页后过滤。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 依赖
- 适用场景
- 使用步骤
- 可选 LDAP 与通知 — 安装、TLS/AD 配置、标准模板、自定义发送
- 核心 API
- HTTP API 契约
- 错误码 — `HaiIamError`
- 常见模式
- 相关 Skills

会话与 refresh 消费记录统一保存在 `hai:iam:sessions:{SHA256(userId)}` 版本化文档，不再使用独立 token 映射/索引。Bearer token 携带不可作为凭据的分区提示和 256 位随机值；客户端必须视为不透明字符串。`Session.version` 用于轮换提交校验。Memory 仅用于单进程测试，生产所有节点共享 Redis（含 Cluster）。并发冲突从最新文档重试，耗尽返回 CONFLICT；旧 refresh 不会产生第二个后继。若响应丢失后结果不明，调用方按失败关闭并重新登录，不假定旧凭据仍有效；全部会话仍受同一用户状态索引管理。

`iam.session.update(token, patch, expectedVersion)` 必须传入读取到的 Session.version。旧版本返回 CONFLICT，已撤销 token 返回 SESSION_NOT_FOUND；不会将旧内容重新写回或覆盖新会话。仅滑动时间戳更新不改变内容版本，允许正常并行请求。批量权限更新绑定首次读取的 token/版本集合，不将迟到数据应用到后续登录；遇到较新内容版本失败，调用方重新查询授权后再提交。

`create({ maxAge })` 可覆盖配置默认时长（正安全整数秒），该值持久化为 `Session.maxAge`；签发和刷新返回的 `expiresIn` 均使用它。滑动续期按该时长延长 access，返回已成功提交的 `lastActiveAt/expiresAt`；迟到请求只延长时间，不缩短较新续期。显式 `update` 的有效日期补丁完整生效并产生新内容版本。客户端根据本次返回的 `expiresIn` 安排刷新，不能硬编码配置默认值。

access 与 refresh 独立到期：用户分区保留至所有 access/refresh 的最晚有效期，但 access 认证只检查自己的 `expiresAt`。access 过期仍可凭未过期、未撤销的 refresh 轮换；每次轮换从新签发时刻重新计算 `refreshTokenMaxAge`，旧 access/refresh 同时失效。删除 access、logoutAll、单设备替换均撤销待刷新记录；仅 revokeRefresh 保留仍有效的 access。刷新路由只校验 refresh，不能先要求旧 access 认证成功。

OTP 发布/消费用单身份 CAS；密码重置最终 SQL 同时核验 resetVersion、credentialVersion、有效期、用户状态并改密/消费令牌。认证与刷新绑定数据库凭据代际；通知第三参数提供 expiresAt/idempotencyKey，接收端持久化幂等，异常是投递未知且不自动重发。

`iam.auth.logout(accessToken)` 直接按完整不透明令牌在用户分区执行一次 CAS，原子撤销 access 与关联 refresh；access 已到期也不能跳过撤销。非法/不存在的令牌幂等成功，缓存读取或提交故障返回失败，恢复后可用同一令牌重试。HTTP 登出只允许撤销所持令牌，不以 userId 定位他人会话，也不要求该 access 仍通过认证。调用方必须检查 `success`；本地 Cookie/安全存储清理不代表服务端撤销已经确认，失败应显示提示。

配置示例用 `IamConfigInput` 表达可省略默认值的输入；需要完整策略快照时用对应 Schema.parse 得到输出类型。应用仅从 `@h-ai/iam` 取公开函数/类型，不深导内部策略与仓储。源码内部示例明确标注上下文，文档示例类型检查不替代真实共享存储和通知集成验证。
