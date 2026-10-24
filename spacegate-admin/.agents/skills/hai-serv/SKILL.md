---
name: hai-serv
description: "为独立 API 服务装配 contract/procedures、认证守卫、Node/Fetch、OpenAPI 与语音 WebSocket。"
---

# hai-serv

transport 仅支持 v2：SM2 发送方签名、独立密钥的 Encrypt-then-HMAC、HTTP 上下文/响应状态绑定及共享原子防重放。GET/HEAD、空 POST 与空响应也认证；无降级开关。TLS 信任边界、5 分钟窗口、nonce 持久化与实现示例见 [参考](reference.md)。

## 能力契约

| 项目     | 契约                                                                                                                    |
| -------- | ----------------------------------------------------------------------------------------------------------------------- |
| 能力     | 使用 @h-ai/serv 装配 contract/procedures、认证守卫、OpenAPI、HTTP 与语音 WebSocket                                      |
| 适用场景 | 独立 HTTP 服务装配、请求守卫或运行时适配                                                                                |
| 输入     | contract、procedures、已初始化 features、ServConfigInput                                                                |
| 输出     | HTTP App、Node listener 或 Fetch handler                                                                                |
| 限制     | 无 init/close；createApp 配置错误可抛出。依赖先初始化，onClose 反序释放。Hono 为内部实现，业务授权由 route guard 执行。 |

## 使用路径

- 在满足包 Node API 依赖的宿主中选择 listen 或标准 Fetch adapter，再从共享 contract 装配 procedures；不要直接暴露 Hono。Node 自动提供 socket 对端；Fetch 通过 app.fetch 的 remoteAddress 元数据提供可信平台对端。http.trustedProxies 默认为空，仅显式代理 IP/CIDR 可以提供转发头；自定义 context 传递完整 input 保留解析后的 ip。
- Storage 默认 procedures 必须注入 authorize 资源策略，使用可信 session 校验全部 key/prefix；缺失策略默认拒绝，混合批量全量授权后才执行。
- AI 默认 feature 以可信 session.userId 绑定 objectId，所有读写和记忆按主体隔离；跨主体访问必须显式 authorizeObject，同名 sessionId 不代表同一用户。
- 认证绑定 route 的 auth/permission/role；启动配置错误与正常请求领域错误分别处理。
- 普通 procedure 的失败 HaiResult 默认通过 HTTP 200 返回，必须检查 success 与 error.code；error.httpStatus 是领域错误元数据。contract 输入校验失败为 HTTP 400，HTTP middleware、刷新 cookie 等响应另按自身契约处理，不能混为同一种状态映射。
- 浏览器 Cookie、CORS、apiPrefix、transport 须与 api-client 同步；语音 ticket 单独核验。
- transport 服务端显式注入受管 serverKeyRing 和共享 keyStore；请求带协商 serverKeyId。私钥只从 Secret/私有环境加载，不放公开配置；轮换、重启与跨节点要求见 hai-crypto。
- 缺失路由返回本地化 HaiResult JSON 404；加密传输保留该状态。默认 feature 可按需选用，contract 与挂载的 procedures 保持一致。

传输加密按实际请求/响应 body 判断空正文，不信任 Content-Type/Length；非空明文不能借缺头、chunked 或伪造零长度旁路，实际空流保留无 input POST 行为。

transport 与 kit/客户端共享固定 UTF-8 字节预算：握手 JSON 16 KiB、公钥 4 KiB、明文 1 MiB、密文 JSON 2 MiB + 32 KiB。原流有界读取，超限立即取消并释放 reader；请求超限 413、响应超限 500。重建 Request 保留 signal 并清空 Hono bodyCache，移除旧长度/编码头；不新增 YAML/环境变量开关。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用步骤
- 核心 API
- HTTP 配置（`ServHttpConfigInput`）
- Feature Procedures（默认实现）
- 常见模式
- 错误码
- httpOnly Cookie 认证
- 测试

标准登出入口 `auth.logout` 从 input.accessToken 或 Authorization Bearer 取得要撤销的完整令牌，不要求 access 仍有效。它不接受 userId 授权或撤销目标；缺少令牌返回失败，未知令牌幂等成功。共享存储故障原样保留失败 HaiResult，Cookie middleware 只对成功结果清理 refresh Cookie。普通 procedure 继续使用既有 HTTP 200 / HaiResult 协议，客户端必须检查 success，不能只检查状态码。

JSDoc 公开例子按实际根入口/features 声明编译；“模块内部上下文”不能照抄为根 API。工厂配置异常、领域 HaiResult 与 HTTP middleware 的状态分别处理；listener.close 仅关闭 HTTP，onClose 为信号关闭回调，业务资源仍由宿主管理。

使用 toFetch 注入时，Request 输入同样应用 RequestInit 覆盖，并按标准构造器拒绝非法/已消费/锁定正文；取消会停止派发或响应等待并传给应用，不回滚副作用。Response 交付后的流由应用监听 request.signal 清理。
