# hai-ai — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

`@h-ai/ai` 统一提供 LLM、工具、MCP、Embedding、Memory、Retrieval/RAG、Knowledge、Context、File、Rerank、Image、Audio 与 A2A 能力。

## 使用边界

- `ai.tools`、`ai.stream` 是纯函数能力，无需 `ai.init()`。
- `ai.llm`、`ai.embedding`、`ai.memory`、`ai.retrieval`、`ai.rag`、`ai.knowledge`、`ai.context`、`ai.image`、`ai.audio`、`ai.a2a` 需要先 `await ai.init(...)`。
- 开发/显式测试未初始化 reldb/vecdb 时，`ai.init()` 使用临时 Store；关闭/进程退出后丢失。
- `NODE_ENV` 或 `HAI_CORE_ENV=production` 要求共享 Store，内置实现先初始化 PostgreSQL/MySQL 与网络 vecdb；自定义 `AIStoreProvider` 必须如实声明 stateScope=shared，不能把本机后端作为生产共享状态。
- 浏览器端不要直接调用 Node-only 能力；通过 `@h-ai/api-client` 或应用自定义 API/SSE endpoint 代理。
- 公共 API 返回 `HaiResult<T>` 时按 `if (!result.success) return result` 处理；不要用 `try/catch` 包裹正常业务错误。

## 初始化与关闭

```ts
import { ai } from '@h-ai/ai'
import { core } from '@h-ai/core'

const init = await ai.init(core.config.get('ai'))
if (!init.success)
  return init

// LLM-only：未初始化 DB 时使用进程内临时 Store
// ... 使用 ai.llm

await ai.close()
```

需要持久化时先校验三份配置，再按依赖顺序初始化。此片段位于应用启动生命周期；应用统一关闭管理器负责释放已获取的依赖，初始化失败时不接收业务请求。

```ts
import { ai, AIConfigSchema } from '@h-ai/ai'
import { core } from '@h-ai/core'
import { reldb, ReldbConfigSchema } from '@h-ai/reldb'
import { vecdb, VecdbConfigSchema } from '@h-ai/vecdb'

const databaseConfig = core.config.validate('db', ReldbConfigSchema)
if (!databaseConfig.success)
  return databaseConfig
const vectorConfig = core.config.validate('vecdb', VecdbConfigSchema)
if (!vectorConfig.success)
  return vectorConfig
const aiConfig = core.config.validate('ai', AIConfigSchema)
if (!aiConfig.success)
  return aiConfig
const database = await reldb.init(databaseConfig.data)
if (!database.success)
  return database
const vectors = await vecdb.init(vectorConfig.data)
if (!vectors.success)
  return vectors
return await ai.init(aiConfig.data)
```

应用的关闭生命周期按依赖反序执行。失败时保留下层依赖并报告错误，待修正后重试关闭；业务异常也须进入同一个关闭管理器。

```ts
import { ai } from '@h-ai/ai'
import { reldb } from '@h-ai/reldb'
import { vecdb } from '@h-ai/vecdb'

await ai.close() // AI 按实际 Promise<void> 契约关闭
const vectorsClosed = await vecdb.close()
if (!vectorsClosed.success)
  return vectorsClosed
return await reldb.close()
```

MySQL 首次初始化允许多个节点并发建索引：仅接受 mysql2 明确的 ER_DUP_KEYNAME/1061，并重新核对列及顺序、非唯一完整列、升序可见 BTREE 和无表达式；已有同名错误结构仍失败。账户需要建表和 INDEX 权限，元数据读取、权限或其他 DDL 错误使 ai.init 返回 CONFIGURATION_ERROR。返回前等待全部已启动 DDL 结束；MySQL DDL 不回滚，失败保留已创建结构，修复权限/结构后再重试。逐步检查初始化结果，失败不接收业务请求，并逆序关闭 AI、vecdb、reldb。

自定义存储：

```ts
await ai.init(core.config.get('ai'), { storeProvider: myProvider })
await ai.close()
```

## 配置要点

```yaml
llm:
  apiKey: ''
  baseUrl: https://api.openai.com/v1
  model: gpt-4o-mini
  api: chat # chat（默认）| responses | anthropic——底层协议，对使用方透明
  timeout: 60000
  tempModelCacheTtl: 600000 # 临时模型客户端缓存 TTL（毫秒，默认 10 分钟）
  models: # 可为单个模型指定协议（responses / anthropic）
    - {id: claude, model: claude-3-5-sonnet-latest, api: anthropic}
  scenarios:
    chat: fast
    reasoning: strong
    embedding: embed

embedding:
  dimensions: 1536
  batchSize: 100

knowledge:
  collection: hai_ai_knowledge
  dimension: 1536
  chunkOptions:
    mode: markdown
    maxSize: 1500
    overlap: 200

# 可选：mem0ai/oss 引擎；无法映射持久化后端时默认 fail-fast
memory:
  provider: mem0
  defaultTopK: 10
  # allowEphemeralFallback: true # 仅开发/明确 E2E，生产禁止实际内存回退
```

约定环境变量优先于 YAML，例如 `HAI_AI_LLM_APIKEY`、
`HAI_AI_LLM_BASEURL`、`HAI_AI_LLM_MODEL`。

`ai.config` 是脱敏快照；`apiKey`、`privateKey`、URL 内嵌凭证会被隐藏。

## 常用 API

| 场景          | 首选 API                                                                                                       | 备注                                                                                                                                                         |
| ------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 非流式对话    | `ai.llm.chat({ messages })`                                                                                    | OpenAI 兼容 Chat Completion                                                                                                                                  |
| 流式对话      | `ai.llm.chatStream({ messages })`                                                                              | 返回 `AsyncIterable`                                                                                                                                         |
| 多协议        | 模型 `api: chat/responses/anthropic`                                                                           | 底层走 Chat Completions / Responses / Anthropic，公共形状不变                                                                                                |
| 请求取消      | `ai.llm.chat({ messages, signal })`                                                                            | 传 `AbortSignal`，`abort()` 立即停上游生成                                                                                                                   |
| 临时模型      | `ai.llm.chat({ messages, tempModel })`                                                                         | 单次请求级临时端点，客户端按 TTL 缓存                                                                                                                        |
| 工具定义      | `ai.tools.define(...)`                                                                                         | Zod schema 转 JSON Schema；`handler(input, ctx)` 第二参为执行上下文                                                                                          |
| 工具注册      | `registry.register/replace`                                                                                    | `register` 默认拒绝同名并返回 HaiResult；只有 `replace` 可显式替换                                                                                           |
| 工具执行      | `registry.execute(tc, { signal, objectId, sessionId, deadline?, timeoutMs?, authorize? })` / `executeAll(...)` | Registry/单次授权必须同时通过；默认串行，支持取消/超时；只有确认无副作用时才 `parallel: true`                                                                |
| MCP 服务      | `createMcpServer(...)`                                                                                         | 按需连接 HTTP/SSE/Stdio transport                                                                                                                            |
| Embedding     | `ai.embedding.embedText(text)`                                                                                 | 批量用 `embedBatch`                                                                                                                                          |
| 记忆          | `ai.memory.extract/recall/injectMemories`                                                                      | `extract(..., { signal })` 可取消整条提取链；用 `objectId` 做主体隔离，`scope` 做业务作用域隔离；scope 隔离越细，`candidateMultiplier` 调大以防漏召回        |
| 记忆作用域    | `ai.memory.scoped({ objectId, scope })`                                                                        | **多租户推荐入口**：所有操作自动绑定 objectId/scope（含归属校验）；`clear` 仅清作用域内                                                                      |
| 记忆管理      | `ai.memory.admin.clearAll({ confirm: true })`                                                                  | 唯一的全局清空入口（需显式确认）；`ai.memory.clear()` 空过滤会被拒绝，防误清全局                                                                             |
| 角色人格      | `ai.persona.save/get/compose`                                                                                  | 定义 AI 身份（systemPrompt + traits），`compose` 组合系统提示词；按 `objectId` 隔离多租户（不传默认 `system` 平台角色），`scope: { personaId }` 关联长期记忆 |
| Retrieval/RAG | `ai.retrieval.retrieve` / `ai.rag.query`                                                                       | Retrieval source 先注册或由配置预置                                                                                                                          |
| Knowledge     | `ai.knowledge.setup/ingest/ask`                                                                                | 入库前会调用 datapipe 清洗分块                                                                                                                               |
| Context       | `ai.context.createManager`                                                                                     | 编排 LLM + Memory + RAG + 压缩；`manager.consolidate()` 把会话固化为长期记忆                                                                                 |
| Context 并发  | `createManager({ scope, concurrency: 'reject' \| 'queue' })`                                                   | 默认单活动生成防消息乱序；活动生成期间新 chat 返回 `CONTEXT_BUSY`（reject）或排队（queue）                                                                   |
| Context 重置  | `await manager.reset({ preserveSystemPrompt?, cancelActiveTurn?, waitForMemoryTasks? })`                       | 异步：终止活动生成、清空消息/摘要/轮次，默认保留系统提示词                                                                                                   |
| 语音识别      | `ai.audio.transcribe` / `transcribeStream`                                                                     | 完整音频返回 `HaiResult`；持续音频输入用 `AudioInputStream` 流式返回临时文本                                                                                 |
| 语音合成      | `ai.audio.synthesize` / `synthesizeStream`                                                                     | `text` 可为字符串或 `AsyncIterable<string>`（可接 LLM 文本流边生成边合成）；完整结果可用 `serializePlayableAudio` 转为浏览器播放负载                         |
| 图片生成      | `ai.image.generate({ prompt, size?, model?, referenceImages? })`                                               | 可选参考图使用字节 + MIME；Provider 协议和临时 URL 下载由模块内部处理                                                                                        |
| A2A           | `ai.a2a.registerExecutor/handleRequest`                                                                        | 延迟初始化 SDK handler                                                                                                                                       |

## LLM + 工具调用

```ts
import { z } from 'zod'

const registry = ai.tools.createRegistry({
  authorize: ({ toolName, context }) => canExecuteTool(context.objectId, toolName),
})
const registered = registry.register(ai.tools.define({
  name: 'get_weather',
  description: '获取天气',
  parameters: z.object({ city: z.string() }),
  handler: async ({ city }) => ({ city, temperature: 20 }),
}))
if (!registered.success)
  return registered

const messages: ChatMessage[] = [{ role: 'user', content: '北京天气？' }]
const objectId = 'user-001'
let result = await ai.llm.chat({ messages, tools: registry.getDefinitions() })

while (result.success && result.data.choices[0]?.finish_reason === 'tool_calls') {
  const assistant = result.data.choices[0].message
  const toolCalls = assistant.tool_calls ?? []
  messages.push(assistant)

  const toolMessages = await registry.executeAll(toolCalls, { objectId })
  if (!toolMessages.success)
    return toolMessages
  messages.push(...toolMessages.data)

  result = await ai.llm.chat({ messages, tools: registry.getDefinitions() })
}
```

## 流式处理

```ts
const processor = ai.stream.createProcessor()
for await (const chunk of ai.llm.chatStream({ messages })) {
  const delta = processor.process(chunk)
  if (delta?.content)
    process.stdout.write(delta.content)
}
const message = processor.toAssistantMessage()
```

## 临时模型（tempModel）

单次请求级别绕过配置注册模型，直接指定端点与凭据；适用于运行时动态切换模型、多租户各带凭据。
`chat` / `chatStream` / `ask` / `askStream` 均支持。未指定字段回退全局 LLM 配置 / 环境变量；
请求显式 `max_tokens` / `temperature` 优先于 `tempModel.maxTokens` / `temperature`。
临时客户端按 `apiKey+baseUrl+timeout` 缓存，TTL 默认 10 分钟（`llm.tempModelCacheTtl` 毫秒，可配置），与常驻模型客户端隔离。

```ts
const result = await ai.llm.chat({
  messages: [{ role: 'user', content: '你好' }],
  tempModel: {
    model: 'claude-3-5-sonnet',
    apiKey: process.env.TEMP_MODEL_API_KEY,
    baseUrl: 'https://temp.endpoint/v1',
    maxTokens: 2048, // 可选
    temperature: 0.3, // 可选
    timeout: 30000, // 可选
  },
})

// ask 便捷方法同样支持
await ai.llm.ask('翻译这段话', { tempModel: { model: 'gpt-4o', apiKey: process.env.TEMP_MODEL_API_KEY } })
```

## MCP 服务器

```ts
import { createMcpServer, StreamableHTTPServerTransport } from '@h-ai/ai'
import { core } from '@h-ai/core'

const server = createMcpServer({ name: 'my-server', version: '1.0.0' })
server.registerTool('search', {
  description: '搜索',
  inputSchema: { query: z.string() },
}, async ({ query }) => ({
  content: [{ type: 'text', text: `Results for ${query}` }],
}))

await server.connect(new StreamableHTTPServerTransport({ sessionIdGenerator: core.id.uuid }))
```

## 记忆、RAG 与知识库

`memory.provider` 默认为 `native`（推荐）：HAI 原生引擎，复用 vecdb/reldb/LLM/Embedding，`extract` 采用 **Mem0 式批量合并**（一次 LLM 调用对整批事实与相关既有记忆做 ADD/UPDATE/DELETE/NONE，支持 `category`）。`mem0` 则直接使用 `mem0ai/oss` 的 `Memory` 引擎：LLM/Embedder 从 `llm` 配置提取（OpenAI 兼容），向量库从底层 vecdb 后端提取（qdrant 复用）。当前适配仅映射带完整连接参数的 `qdrant`；生产环境（`NODE_ENV` 或 `HAI_CORE_ENV` 为 `production`）即使设置 `memory.allowEphemeralFallback: true` 也拒绝实际内存回退。临时回退仅限开发或明确的 `HAI_E2E=1` / `HAI_E2E_MOCK=1`；缺少后端同样不能在生产启动。两者提供同一组 `ai.memory.*` 操作。

Mem0 的 `update` 使用 SDK 原地修改文本与元数据，保持 `id` 和创建时间。SDK 查询使用 `filters.user_id`，写入和整主体删除使用 `userId`。SDK 无列表游标：`list` / `listPage` / 条件 `clear` 最多支持每主体 1000 条原始候选（含过期条目），多取一条探测完整性；超限返回 `MEMORY_LIST_INCOMPLETE`（906），不会伪造 total 或执行部分条件删除。候选完整时按 UTC 日期剔除过期项，再按类型/scope 过滤和分页；仅 `objectId` 的清理使用 SDK `deleteAll`，不受列表上限限制。公共空条件清理被拒绝；跨主体全清仅限受信管理员显式 `admin.clearAll({ confirm: true })`。

Mem0 3.3.1 的 Qdrant 适配仍依赖 `QdrantClient.search`；项目将 `@qdrant/js-client-rest` 精确锁定为受测的 `1.18.0`，AI 的可选 peer 也声明此版本。启动会检查该公开方法，缺失时明确配置失败；不要覆盖为已移除此方法的 1.19。

Mem0 的 pgvector 组合在所有环境拒绝启动：当前 SDK 将 PostgreSQL 连接保存在私有对象中，未提供公开关闭入口，框架无法履行资源清理契约。需要 pgvector 时选择 `memory.provider: native`；该原生后端保持支持。`disableHistory: true` 使用 SDK 无资源的历史占位实现，不创建历史 SQLite 文件。工厂通过公开的一条有界只读查询等待实际后端 ready 后才发布成功。

`add` / `extract` 的 SDK 回执不含完整业务字段，框架写后读取权威记录再返回；并发提取等待全部在途写入结束。若写入或写后读失败，`error.ext.requiresEffectReconciliation=true`，`memoryIds` 保存已知写入 ID（回执丢失时可能为空）。应按主体核对实际记忆后决定恢复，不能盲目重做 add/extract；框架不自动重放。

```ts
const enriched = await ai.memory.injectMemories(messages, { objectId: 'user-001', topK: 5 })
if (!enriched.success)
  return enriched

const compressed = await ai.compress.tryCompress(enriched.data, { strategy: 'hybrid', maxTokens: 4000 })
if (!compressed.success)
  return compressed

const answer = await ai.rag.query('核心架构是什么？', { sources: ['docs'], topK: 5 })
```

`tryCompress` 的成功结果一定满足 `compressedTokens <= maxTokens`。如果永久 system 指令、最近消息或最新 user 输入自身已超过预算，会返回 `HaiAIError.CONTEXT_BUDGET_EXCEEDED`；调用方应停止本次模型请求或提高预算，不能继续发送原始超长消息。

知识库流程：

```ts
const setup = await ai.knowledge.setup()
if (!setup.success)
  return setup

const ingest = await ai.knowledge.ingest({
  documentId: 'doc-001',
  content: markdownText,
  title: '产品手册',
  enableEntityExtraction: true,
})
if (!ingest.success)
  return ingest
if (ingest.data.cleanupPending) {
  const repair = await ai.knowledge.repair({ offset: 0, limit: 100 })
  if (!repair.success)
    return repair
  // 大集合继续分页，并将 repair 全页扫描纳入定期维护。
}

const answer = await ai.knowledge.ask('核心架构是什么？')
```

## Context 管理器

`scope` 必填；共享 scope 用版本化聚合记录保存消息、摘要、owner、轮次和 pending。所有消息、对外 delta 与提交即时持久化。同步 getters 仅返回本管理器最近一次成功读写的独立快照，读取其他节点修改前调用 `await manager.reload()` 或 `restoreManager()`。`save()` 等待本节点记忆提取、刷新快照并同步目录，不能覆盖远端版本。`markTurnSpeaking` 为异步持久化方法。

共享单活动生成租约为存储端时钟30秒，每10秒续租，最终 CAS 同时检查版本、owner 和截止时间。过期生成仅 interrupted，不自动重放 LLM/工具/记忆；manual pending 持久保存，可跨节点/重启按 turnId 提交。相同终态与真实文本重复提交幂等，内容冲突失败。queue 可取消轮询，不保证FIFO。reset 更换共享纪元，删除保留永久墓碑且 scope 不复用。外部 LLM/工具/本地后台记忆不与上下文跨系统原子提交；崩溃后的未知副作用必须由应用核对。

自定义 `AIRelStore` 必须提供版本化读取、原子 CAS 和存储端 `currentTime()`；普通 save/saveMany 也推进版本。CAS 缺失版本只允许创建，notAfter 在最终持久化时严格检查，存储故障抛错不能当空数据；业务 HaiResult 边界负责转失败。默认 DB schema 含 version 列，不自动迁移旧表；SQLite 仅用于开发/测试，生产使用 PostgreSQL/MySQL。

```ts
const managerResult = ai.context.createManager({
  scope: { objectId: 'user-001', sessionId: 'sess-001' },
  systemPrompt: '你是一个友好的助手。',
  compress: { auto: true, strategy: 'hybrid', maxTokens: 8000 },
  memory: { enable: true, enableExtract: true },
})
if (!managerResult.success)
  return managerResult

const reply = await managerResult.data.chat('你好')
const saved = await managerResult.data.save()
if (!saved.success)
  return saved
```

### 真实对话状态（Conversation Commit Layer）

`turnCommit: 'manual'` 时，`chat` / `chatStream` 不自动写入生成文本，只返回 `turnId`；由调用方提交**实际发生的内容**（TTS 播放、被打断等场景）。`chatStream` 在调用上游模型前就产出 `turn_started`（事件序列 `turn_started → delta* → done`，中途取消 `→ cancelled`），取消时保留 `turnId` 与已生成文本：

```ts
const manager = ai.context.createManager({ turnCommit: 'manual', scope })
if (!manager.success)
  return manager
const m = manager.data
const controller = new AbortController()
for await (const ev of m.chatStream('展开讲讲', { signal: controller.signal })) {
  if (ev.type === 'delta')
    feedTts(ev.text)
  else if (ev.type === 'done')
    await m.commitTurn(ev.turnId) // 完整提交
  else if (ev.type === 'cancelled')
    await m.interruptTurn(ev.turnId, { text: spokenSoFar }) // 只提交播放出去的部分
}
```

- 只有 `committed` 的内容进入上下文与记忆；`getTurns()` 可观测 `generated` / `committed` / `status`。
- 主持人抢话时可在 `turn_started` / 迭代过程中随时 `interruptTurn`；轮次进入终态后即便上游流恰好正常结束，`chatStream` 也**不会再产出 `done`**，业务层不会误判为正常完成。

## 语音（Audio）

`ai.audio` 提供 ASR/TTS，Provider（OpenAI / MiMo / Qwen / 豆包）作为内部实现，凭据未配置时回退平台环境变量。调用方只表达「音频还是文本、完整还是流式」，不感知 WebSocket / SSE / 厂商事件。LLM 与语音模型确认共用凭据时，可显式设置 `inheritLlmApiKey: true`；该选项默认关闭，避免跨供应商误用密钥。

```yaml
# ai.init 配置片段
audio:
  # 仅当语音服务与 LLM 确认共用同一密钥时启用
  inheritLlmApiKey: true
  models:
    - {id: asr, provider: qwen, model: qwen3-asr-flash-realtime, operations: [transcribe]}
    - {id: tts, provider: qwen, model: qwen3-tts-flash-realtime, operations: [synthesize]}
  transcribeModel: asr
  synthesizeModel: tts
```

```ts
// 完整识别 / 合成（HaiResult）
const t = await ai.audio.transcribe({ audio: { data: wavBytes, format: 'wav' }, language: 'zh' })
if (!t.success)
  return t
const s = await ai.audio.synthesize({ text: '欢迎', voice: 'Cherry', format: 'pcm16' })

// 实时识别：持续音频输入 → 增量文本（chunk.final 标记语句最终态，可直接覆盖临时文本）
for await (const chunk of ai.audio.transcribeStream({ audio: { chunks: micChunks, format: 'pcm16', sampleRate: 16000 } })) {
  updateTranscript(chunk.text, chunk.final)
}

// 分段合成：稳定 ID 关联文本与音频，支持 AbortSignal 打断
const controller = new AbortController()
for await (const event of ai.audio.synthesizeStream({ text: { id: 'seg-1', text: '欢迎。' }, voice: 'Cherry', signal: controller.signal })) {
  if (event.type === 'segment_started')
    prepareDecoder(event.format, event.sampleRate, event.channels) // 真实输出格式来自服务端解析后的 Provider 输出
  else if (event.type === 'audio')
    await player.write(event.data)
}
```

需要通过 API 返回浏览器可播放的 Base64 音频时，调用 `serializePlayableAudio(result.data)`；该函数会将 `pcm16` 封装为 WAV，并透传 WAV/MP3，`opus` 不支持该转换。

```ts
import { serializePlayableAudio } from '@h-ai/ai'

const synthesized = await ai.audio.synthesize({ text: '欢迎参加访谈。' })
if (!synthesized.success)
  return synthesized

const playable = serializePlayableAudio(synthesized.data)
if (!playable.success)
  return playable
```

- 流式方法是 `AsyncIterable`，迭代期间的连接/协议/上游错误会终止迭代（抛出），不返回 `HaiResult`。
- `segment_started` 携带服务端解析 Provider 后的真实输出参数（`format` / `sampleRate?` / `channels?`）；播放器据此解码，不按请求参数猜测格式。
- 平台不支持的输入方式（如 OpenAI/MiMo 的持续音频输入）会抛 `AUDIO_UNSUPPORTED_INPUT`，不伪装成实时。
- 浏览器/移动端经 `@h-ai/serv` 统一语音 WebSocket 入口访问；客户端用已登录 HTTP 请求获取短期一次性 ticket，`createAudioClient({ url, getTicket })` 通过独立配置的 `getTicket` 建连，不把 IAM access token 放入 URL。浏览器客户端区分正常结束、取消（`AUDIO_CANCELLED`）与 `end` 前异常断连（`AUDIO_CONNECTION_FAILED`）；服务端 error 帧保留领域错误码，`synthesize` 不返回未完成的部分音频。

### 本地 CPU 模型服务

hai-framework 源码仓库提供 `pnpm model <name> --device cpu` 与 `pnpm model:run <name>`，默认优先使用 Podman，可构建/启动 `faster-whisper-large-v3`、`indextts-2.5`、`qwen3-4b`。三套权重均优先从清单锁定的 ModelScope revision 下载并复用容器 volume，Hugging Face 只作回退。Whisper/IndexTTS 仍配置为 `audio.models[].baseUrl`，Qwen3-4B 仍配置为 OpenAI-compatible `llm.models[].baseUrl`；部署位置不进入 Provider 名称。离线部署使用准备权重后的 bundled 镜像。

## 文生图（Image）

`ai.image` 只暴露稳定的提示词、模型别名、像素尺寸、可选参考图和图片字节；OpenAI、Google、Qwen、Seedream 与 Pollinations 的请求结构、鉴权、响应解析和临时 URL 下载均封装在内部 Provider。

```yaml
image:
  maxImageBytes: 20971520 # 单张输出/参考图上限；最大100MiB
  maxImages: 4 # 1–16，URL串行下载；模型timeout覆盖整个生成/下载
  models:
    - {id: free, provider: pollinations, model: zimage}
    - {id: openai, provider: openai, model: gpt-image-2}
    - {id: google, provider: google, model: gemini-3.1-flash-image-preview}
    - {id: qwen, provider: qwen, model: qwen-image-2.0-pro}
    - {id: seedream, provider: seedream, model: doubao-seedream-5-0-260128}
  generateModel: free
```

LLM、Audio、Image 的可选密钥统一使用 `OptionalSecretSchema`：配置中的 YAML `null`、空字符串和纯空白字符串均表示未配置。语音密钥优先级为模型条目 → 启用继承后的 LLM 密钥 → 平台环境变量，业务启动代码不应再遍历配置修补 `apiKey`。

```ts
const image = await ai.image.generate({
  prompt: '水墨风格的未来城市，清晨薄雾',
  size: { width: 1024, height: 1024 },
  referenceImages: [{ data: referenceBytes, mimeType: 'image/png' }],
})
if (!image.success)
  return image
await saveImage(image.data.images[0].data, image.data.images[0].mimeType)
```

- `Pollinations` 提供可免费起步的开发额度，但仍需 API key；不要把“免费额度”描述为匿名或无限调用。
- 应用应通过 `core.init({ configDir: './config' })` 加载 `_core.yml` / `_ai.yml`，再以 `core.config.validate('ai', AIConfigSchema)` 校验并传给 `ai.init`；不要在启动代码中重复拼装模型、端点和超时，也不要传入 `logging` 覆盖 `_core.yml`。
- Pollinations 的 HTTP 402 表示账户余额或该 API Key 的预算耗尽；401 是密钥无效，403 是权限或模型访问被拒。诊断日志记录厂商、模型、状态、上游错误码和请求 ID，不记录密钥、提示词或图片内容。
- Qwen 等返回的临时图片 URL 会在 Provider 内立即下载，公共结果不泄漏易过期 URL。
- 参考图统一使用 `{ data: Uint8Array, mimeType: 'image/...' }`；不要把厂商 URL、multipart 或 Base64 结构泄漏到业务层。
- Provider 请求/响应格式变更时，先更新契约测试，再修改实现。

## SvelteKit API 端点模式

```ts
export const POST = kit.handler(async ({ request }) => {
  const data = await kit.validate.body(request, ChatSchema)
  const result = await ai.llm.chat({ messages: data.messages })
  if (!result.success)
    return kit.response.error(result.error.code, result.error.message)
  return kit.response.ok(result.data)
})
```

## 质量与安全检查

- API Key 只从配置 / 环境变量读取，禁止写死。
- 工具 handler 必须校验输入；用户可见错误走 i18n/错误码。
- 模型输出、Prompt 和检索内容均不可信；不要把用户内容拼入不可覆盖的系统规则。Zod 只保证参数形状，高权限工具还必须在 handler 内校验身份、租户、资源归属和配额。
- 只向模型注册允许自动执行的工具；写操作默认要求业务侧确认或幂等键，不能把“模型请求调用”当作授权。
- 批量导入、批量 embedding 用批量 API，避免 await-in-loop。
- 文件解析/OCR 失败按 `HaiAIError.FILE_*` 返回，不抛业务异常。
- A2A 认证失败返回 `A2A_AUTH_FAILED`；`callRemoteAgent` 只依赖 `ai.init()`，无需注册本地 executor。远端 URL 仅接受 HTTP(S) 且不得内嵌凭据，应用仍须配置 origin 白名单与出口网络策略。不要记录 token、apiKey、私钥、完整 headers 或带 query 的 URL。
- A2A 入站日志保存并重验租约后才执行业务，出站日志在已有有界队列内完成后才发布；日志失败产生 failed/interrupted Task，不重放相同 receipt。日志不是交付回执，按 requiresEffectReconciliation 核对可能已执行的效果；listMessages 失败为安全的 A2A_LIST_MESSAGES_FAILED（984）。
- A2A 任务与事件只有共享 journal 一套权威，不装配旧 TaskStore；push notification 配置和 authenticated extended card RPC 返回 SDK 标准不支持/未配置错误。应用通过公开 executor 接口接入，不导入内部 handler/Store 装配类型。
- Context 的 flush 会报告本节点后台提取异常及失败 HaiResult；同轮并发调用共享一次报告，返回即确认该批，后续只观察新任务。save/reset(waitForMemoryTasks)/consolidate 传播失败并停止后续写入，pendingMemoryTasks=0 不代表成功，不自动重试未知副作用。
