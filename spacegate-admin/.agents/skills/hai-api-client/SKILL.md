---
name: hai-api-client
description: "接入或排查 @h-ai/api-client typed API、登录 Token、自动刷新与加密传输；适用于 Web、SSR 和原生客户端。"
---

# hai-api-client

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/api-client 调用 typed API，管理 Token、刷新与加密传输 |
| 适用场景 | 跨端调用共享 HTTP API 或排查认证刷新 |
| 输入 | baseUrl、共享 contract、TokenStorage、可选 transport.crypto |
| 输出 | typed client；领域响应为 HaiResult |
| 限制 | 先 init，结束 close；浏览器 Cookie 与服务端配置匹配，原生端使用安全 TokenStorage。不要在多用户 SSR 中共享带用户 token 的单例。 |

## 使用路径

1. 读取服务端 contract、apiPrefix、CORS、Cookie 与 transport 配置，选择默认客户端或 `apiClient.create(contract)`。
2. 浏览器使用 Cookie/内存；SSR 按请求创建客户端，原生使用安全 TokenStorage，不跨用户复用单例。
3. 检查 init 与每次业务调用的 `success`。登录成功后设置 tokens；优先使用 `createAuthActions` 连接认证 UI。
4. 同步服务端与客户端的刷新、加密和错误处理；切换配置前 close，任务结束释放客户端。
5. 验证登录、刷新重试、失败、退出与会话恢复。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [使用步骤](reference.md#使用步骤)
- [Token Storage 适配器](reference.md#token-storage-适配器)
- [核心 API](reference.md#核心-api)
- [常见模式](reference.md#常见模式)
- [配置驱动认证页面](reference.md#配置驱动认证页面)

普通请求与 Token 刷新自动将当前 `core.i18n` 语言发送为 `Accept-Language`；切换语言时同步 core。`headers` 中显式配置的同名头优先。
