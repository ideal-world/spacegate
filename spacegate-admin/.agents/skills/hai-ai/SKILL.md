---
name: hai-ai
description: "接入 @h-ai/ai 的 LLM、工具/MCP、RAG、会话记忆、图像、语音及 A2A。"
---

# hai-ai

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 接入 @h-ai/ai 的 LLM、工具/MCP、RAG、会话记忆、图像、语音及 A2A |
| 适用场景 | AI 应用服务端集成或浏览器代理接入 |
| 输入 | AIConfigInput、消息/音频/图片、scope、AbortSignal |
| 输出 | 领域操作为 HaiResult；流为 AsyncIterable，迭代可抛错 |
| 限制 | tools/stream 无需 init；其他领域先初始化。生产须先初始化共享 PostgreSQL/MySQL + 网络 vecdb 或传 stateScope=shared 的 Provider；临时 Store 仅开发/测试，关闭后丢失。工具调用仍需业务授权。 |

## 使用路径

- 先选择完整结果还是流式 API；不要对 AsyncIterable 取 `.success`。流取消/错误须结束播放器和连接。
- Memory/Context 用 objectId/sessionId 隔离主体；Context scope 必填，跨节点读先 reload/restore，manual pending 可跨节点按 turnId 幂等提交实际文本。共享 Store 必须实现版本 CAS/存储端期限；过期生成不自动重放外部副作用。
- Mem0 生产须实际映射持久 Qdrant；内存回退开关不能绕过。列表/条件清理最多 1000 条主体原始候选，超限检查 `MEMORY_LIST_INCOMPLETE`；禁止把截断结果当完整 total。
- HTTP AI 使用 `@h-ai/api-client` 的 typed 领域方法；A2A 由应用定义并绑定自己的 contract；`@h-ai/ai/client` 只提供独立 `createAudioClient`，不再封装字符串路径 HTTP 或重复 SSE helper。
- 语音客户端用一次性 ticket，不能把 IAM access token 放 URL；按 segment_started 的真实格式解码。
- 压缩超过硬预算须停止模型请求；不能失败后发送原始超长消息。

## 按需参考

详细配置、API 签名、错误码和范本在 [guide.md](guide.md)。按下列标题定位所需小节，不默认通读；用例中的业务变量需结合当前应用补齐。

- 使用边界
- 初始化与关闭
- 配置要点
- 常用 API
- LLM + 工具调用
- 流式处理
- 临时模型（tempModel）
- MCP 服务器
- 记忆、RAG 与知识库
- Context 管理器
- 语音（Audio）
- 文生图（Image）
- SvelteKit API 端点模式
- 质量与安全检查

补充类型/错误码与范本见 [reference.md](reference.md)，仅在上述用法不足时读取。

公开 API 示例以安装包的实际入口为准；框架源码中的内部工厂例子仅供模块维护。HaiResult 调用先检查 success，流与浏览器 AudioClient 按显式抛错契约处理；Context flush 失败和 A2A 未知外部效果不得盲目重放。

Node 自定义 SSE 使用 ai.stream 的统一编解码，不用 split/trim 手拆上游事件；encoder 的非法元数据会显式抛错。Anthropic/MiMo 的真实 HTTP 流在退出和取消时释放正文，EOF 不伪造未完成事件，不自动重连或重放业务。
