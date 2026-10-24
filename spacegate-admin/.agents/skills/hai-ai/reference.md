# @h-ai/ai reference

本文件承载 `SKILL.md` 中不常用但仍需要保留的 API 参考。优先读取 `SKILL.md`；只有当任务需要完整方法清单、错误码或长示例时再读取本文件。

## 初始化与依赖

开发/显式测试未初始化 reldb/vecdb 时使用临时 Store；两者就绪时使用 DB Provider。`NODE_ENV` 或 `HAI_CORE_ENV=production` 要求 stateScope 为 shared，内置实现必须为 PostgreSQL/MySQL + 网络 vecdb；本地后端初始化失败，自定义 Provider 必须兑现存储时钟、CAS 与最终截止条件。

MySQL 首次初始化允许多个节点并发建索引：仅接受 mysql2 明确的 ER_DUP_KEYNAME/1061，并重新核对列及顺序、非唯一完整列、升序可见 BTREE 和无表达式；已有同名错误结构仍失败。账户需要建表和 INDEX 权限，元数据读取、权限或其他 DDL 错误使 ai.init 返回 CONFIGURATION_ERROR。返回前等待全部已启动 DDL 结束；MySQL DDL 不回滚，失败保留已创建结构，修复权限/结构后再重试。逐步检查初始化结果，失败不接收业务请求，并逆序关闭 AI、vecdb、reldb。

| 能力 | 是否需要 `ai.init()` | 依赖 |
| --- | --- | --- |
| `ai.tools` | 否 | 无 |
| `ai.stream` | 否 | 无 |
| `ai.llm` | 是 | OpenAI 兼容 LLM 配置 |
| `ai.embedding` | 是 | LLM/Embedding 模型配置 |
| `ai.memory` | 是 | Store + 可选 Embedding |
| `ai.retrieval` / `ai.rag` | 是 | Embedding + Store |
| `ai.knowledge` | 是 | LLM + Embedding + Store + datapipe |
| `ai.context` | 是 | LLM + Store + 可选 Memory/RAG/Reasoning |
| `ai.a2a` | 是 | 接收请求需本地 executor；调用远端无需注册 executor |

关闭必须使用 `await ai.close()`，这样才能确定释放自定义 `AIStoreProvider.close()`。

## LLM

| 方法 | 说明 |
| --- | --- |
| `chat(options)` | 非流式 Chat Completion |
| `chatStream(options)` | 流式 Chat Completion |
| `ask(query, options?)` | 返回纯文本的简易问答 |
| `askStream(query, options?)` | 流式纯文本问答 |
| `listModels()` | 查询可用模型 |
| `getHistory(scope, options?)` | 查询会话历史 |
| `listSessions(objectId)` | 查询主体会话 |

```ts
const result = await ai.llm.chat({
  model: 'fast',
  messages: [
    { role: 'system', content: '你是一个翻译助手' },
    { role: 'user', content: '你好世界' },
  ],
})
if (result.success) {
  const reply = result.data.choices[0]?.message.content ?? ''
}
```

## Tools

- `ai.tools.define({ name, description, parameters, handler })`
- `ai.tools.createRegistry({ authorize? })`
- `registry.register(tool)` / `registry.registerMany(tools)`：返回 `HaiResult<void>`，默认拒绝同名
- `registry.replace(tool)`：显式替换已存在工具
- `registry.unregister(name)`
- `registry.getDefinitions()`：传给 LLM 的工具定义
- `registry.execute(toolCall, { signal, objectId, sessionId, deadline?, timeoutMs?, authorize? })` / `registry.executeAll(toolCalls, { parallel?, ...ctx })`；默认串行；Registry/单次授权均 fail-closed；handler `(input, ctx)` 可响应取消/超时（默认 30s）

工具 handler 输入由 Zod schema 约束，外部输入不要绕过 schema。

## MCP

### 独立服务器

```ts
const mcp = createMcpServer({ name: 'my-server', version: '1.0.0' })
mcp.registerResource('config', 'config://app', { description: '应用配置' }, async uri => ({
  contents: [{ uri: uri.href, text: '{}' }],
}))
await mcp.connect(new StdioServerTransport())
```

### `ai.mcp`

这是进程内注册表，不会自动绑定独立 MCP Server 的 transport。工具注册时编译 JSON Schema、调用时校验输入；非法输入不执行 handler。资源必须恰好含 text/blob 之一；提示词必填参数必须是自身字符串属性，输出使用 SDK 消息类型（含 image/audio）。同名注册覆盖，缺失 requestId 自动补 UUID。已移除未使用的 MCPProvider 与 AIMCPFunctionsDeps 类型；使用 MCPOperations 即可。

| 方法 | 说明 |
| --- | --- |
| `registerTool(definition, handler)` | 注册内置工具 |
| `callTool(name, args, context?)` | 调用工具 |
| `registerResource(resource, handler)` | 注册资源 |
| `readResource(uri)` | 读取资源 |
| `registerPrompt(prompt, handler)` | 注册提示词 |
| `getPrompt(name, args)` | 生成提示词消息 |

## Embedding

Embedding OpenAI 客户端按 `apiKey + baseURL + timeout` 缓存；同一 key 指向不同网关时不会复用错误客户端。

| 方法 | 说明 |
| --- | --- |
| `embed(request)` | 支持单条/批量输入 |
| `embedText(text)` | 返回单条向量 |
| `embedBatch(texts)` | 返回向量数组 |

## Memory

| 方法 | 说明 |
| --- | --- |
| `extract(messages, options?)` | 从对话提取记忆并存储；`options.signal` 可取消事实抽取及 native 对账调用 |
| `add(entry)` | 手动添加记忆 |
| `get(memoryId, accessScope?)` / `update(memoryId, updates, accessScope?)` | 读取/更新（传 `accessScope: { objectId, scope? }` 做归属校验，不匹配→ `MEMORY_NOT_FOUND`） |
| `recall(query, options?)` | 检索相关记忆 |
| `injectMemories(messages, options?)` | 注入 system 记忆上下文 |
| `remove(memoryId, accessScope?)` / `clear(options?)` | 删除（`accessScope` 同上） |
| `list(options?)` / `listPage(options?)` | 列表/分页 |

推荐所有记忆操作带 `objectId`，避免不同用户或 Agent 混写。

Memory 后端通过 `memory.provider` 选择：

- `native`（默认）：HAI 原生引擎，复用 vecdb/reldb/LLM/Embedding。`extract` 采用 Mem0 式批量合并（ADD/UPDATE/DELETE/NONE + `category`）；`maxEntriesPerObject`（单主体配额）/`maxEntriesGlobal`（全局上限）/`recencyDecay`/`embeddingEnabled`/`writebackRelatedTopK` 作用于此后端；淘汰按 `objectId` 分区。
- `mem0`：直接使用 `mem0ai/oss` 的 `Memory` 引擎（嵌入式）。LLM/Embedder 从 `llm` 配置提取（OpenAI 兼容，场景模型 `extraction` / `embedding`）；向量库从底层 vecdb 后端提取（`storeProvider.getVectorBackend()`），qdrant 复用同一后端；当前适配仅映射带完整连接参数的 `qdrant`；生产环境（`NODE_ENV` 或 `HAI_CORE_ENV` 为 `production`）即使设置 `memory.allowEphemeralFallback: true` 也拒绝实际内存回退。临时回退仅限开发或明确的 `HAI_E2E=1` / `HAI_E2E_MOCK=1`；缺少后端同样不能在生产启动。历史默认禁用。
- 两者对外 API（extract/recall/injectMemories/add/update/get/remove/list/listPage/clear）完全一致；`objectId` 隔离不同主体的记忆，`scope`（key-value）用于业务作用域隔离（recall/list/listPage/clear 均严格过滤）。mem0 后端 `extract` 在框架层分类打分后以 `infer:false` 写入（保留 `hai_type`/`hai_importance`），`recall` 支持 `types`/`recencyWeight`，与 native 一致；Mem0 的 `update` 使用 SDK 原地修改文本与元数据，保持 `id` 和创建时间。SDK 查询使用 `filters.user_id`，写入和整主体删除使用 `userId`。SDK 无列表游标：`list` / `listPage` / 条件 `clear` 最多支持每主体 1000 条原始候选（含过期条目），多取一条探测完整性；超限返回 `MEMORY_LIST_INCOMPLETE`（906），不会伪造 total 或执行部分条件删除。候选完整时按 UTC 日期剔除过期项，再按类型/scope 过滤和分页；仅 `objectId` 的清理使用 SDK `deleteAll`，不受列表上限限制。公共空条件清理被拒绝；跨主体全清仅限受信管理员显式 `admin.clearAll({ confirm: true })`。

Mem0 3.3.1 的 Qdrant 适配仍依赖 `QdrantClient.search`；项目将 `@qdrant/js-client-rest` 精确锁定为受测的 `1.18.0`，AI 的可选 peer 也声明此版本。启动会检查该公开方法，缺失时明确配置失败；不要覆盖为已移除此方法的 1.19。

适配器通过 SDK 公开的 `client` 配置注入独享客户端；Mem0 的 Qdrant 写入与删除均使用 `wait: true`，等待服务端应用完成后才继续写后读或分批清理。

Mem0 的 pgvector 组合在所有环境拒绝启动：当前 SDK 将 PostgreSQL 连接保存在私有对象中，未提供公开关闭入口，框架无法履行资源清理契约。需要 pgvector 时选择 `memory.provider: native`；该原生后端保持支持。`disableHistory: true` 使用 SDK 无资源的历史占位实现，不创建历史 SQLite 文件。工厂通过公开的一条有界只读查询等待实际后端 ready 后才发布成功。

`add` / `extract` 的 SDK 回执不含完整业务字段，框架写后读取权威记录再返回；并发提取等待全部在途写入结束。若写入或写后读失败，`error.ext.requiresEffectReconciliation=true`，`memoryIds` 保存已知写入 ID（回执丢失时可能为空）。应按主体核对实际记忆后决定恢复，不能盲目重做 add/extract；框架不自动重放。

## Retrieval / RAG

```ts
await ai.retrieval.addSource({
  id: 'docs',
  name: '产品文档',
  collection: 'doc_vectors',
  topK: 5,
  minScore: 0.7,
})

const result = await ai.rag.query('核心架构是什么？', {
  sources: ['docs'],
  topK: 5,
})
```

`retrieve` 会并发检索目标 source 并按 score 合并排序；无 source 时返回 `RETRIEVAL_SOURCE_NOT_FOUND`。

## Knowledge

| 方法 | 说明 |
| --- | --- |
| `setup(options?)` | 初始化 collection / 表 / 索引 |
| `ingest(input)` | 文档入库 |
| `ingestFile(input)` | 从文件导入（Node.js） |
| `ingestBatch(inputs, onProgress?)` | 批量导入 |
| `retrieve(query, options?)` | 实体增强检索 |
| `ask(query, options?)` | 知识库问答 |
| `findByEntity(entityName, options?)` | 实体关联文档 |
| `listEntities(options?)` / `listDocuments(options?)` | 列表 |
| `removeDocument(documentId, options?)` | 删除文档和关联 |
| `repair({ collection?, offset?, limit? })` | 有界恢复过期暂存与失败清理，删除集合后也可调用 |

入库链路：clean/chunk → embedding/完整实体提取 → 持久暂存清单 → vecdb upsert → 单文档 CAS 发布完整正文、实体和元数据。每次产生唯一 revision/chunk ID，相同文档并发重建只允许一个 writer；失败保留旧版，短文档重建不显示旧尾块。向量检索只保留当前发布版，切换期间重试，持续冲突或候选扩展超限返回错误。

`ingest` 成功返回 `revision`、`cleanupPending`。后者为 true 表示完整新版已经发布但旧向量删除失败，须检查并安排 repair，不能当作全部清理完成。`setup` 自动扫描前 100 份清单；应用定期分页 repair（offset≥0，limit 1–1000，默认100），根据 processed/total 覆盖全部页。存储端时钟的 5 分钟暂存过期后旧 owner 不能发布；废弃写入的永久补偿墓碑用于反复清理迟到外部写入，不能随日志删除。SQL 与 vecdb 无跨库事务，故障/重启后的清理依赖持久记录及重复扫描。

删除先撤销可见性，清理失败返回错误并保留补偿信息。集合删除保留物理向量集合与代际墓碑，重新 setup 不会恢复旧代数据。默认 Store 的正文/实体来自同一已发布快照；自定义 KnowledgeStore 必须实现相同发布、删除和补偿契约。Qdrant 写入/删除等待应用，候选上限使用精确计数；LanceDB 使用持久主键原子 merge。

使用 LanceDB 时，集合维度与 metric 随 Arrow schema 持久化；连接 metric 只影响新集合。重启后复用同一持久目录，embedding 维数改变时重建匹配集合；多个节点各自的本地路径不会自动共享，分布式 Knowledge 推荐服务端向量后端。

Knowledge `retrieve/ask` 与默认 DB Store 的 filter 透传向量 Provider，校验/查询失败必须传播（Knowledge 为 KNOWLEDGE_RETRIEVE_FAILED，ask 为 RAG_FAILED），不能当作空成功结果。LanceDB 仅支持顶层字面量键的 string/有限 number/boolean/null AND 匹配，例如 `{ tenant: 'acme', active: true }`；null 不匹配缺失字段，对象/数组/undefined 返回错误。过滤在向量 topK 前执行；其它后端的 null/复杂值语义须独立核对。LanceDB 原生 JSON 字段保留嵌套 JSON，但旧 Utf8 metadata 集合必须重建后才能写入或过滤。

## Context

`scope` 必填；共享 scope 用版本化聚合记录保存消息、摘要、owner、轮次和 pending。所有消息、对外 delta 与提交即时持久化。同步 getters 仅返回本管理器最近一次成功读写的独立快照，读取其他节点修改前调用 `await manager.reload()` 或 `restoreManager()`。`save()` 等待本节点记忆提取、刷新快照并同步目录，不能覆盖远端版本。`markTurnSpeaking` 为异步持久化方法。

共享单活动生成租约为存储端时钟30秒，每10秒续租，最终 CAS 同时检查版本、owner 和截止时间。过期生成仅 interrupted，不自动重放 LLM/工具/记忆；manual pending 持久保存，可跨节点/重启按 turnId 提交。相同终态与真实文本重复提交幂等，内容冲突失败。queue 可取消轮询，不保证FIFO。reset 更换共享纪元，删除保留永久墓碑且 scope 不复用。外部 LLM/工具/本地后台记忆不与上下文跨系统原子提交；崩溃后的未知副作用必须由应用核对。

自定义 `AIRelStore` 必须提供版本化读取、原子 CAS 和存储端 `currentTime()`；普通 save/saveMany 也推进版本。CAS 缺失版本只允许创建，notAfter 在最终持久化时严格检查，存储故障抛错不能当空数据；业务 HaiResult 边界负责转失败。默认 DB schema 含 version 列，不自动迁移旧表；SQLite 仅用于开发/测试，生产必须 PostgreSQL/MySQL 与共享向量服务。

`ai.context.createManager(options)` 返回有状态管理器，适合多轮对话。

常用选项：

- `scope: { objectId, sessionId }`（必填）
- `systemPrompt`
- `compress: { auto, strategy, maxTokens, preserveLastN }`
- `memory: { enable, enableExtract, scope, types, minImportance, topK, position, extractionModel, extractionSystemPrompt }`（scope/types 等完整透传给 Memory 的 recall/extract）
- `rag: { enable, sources, topK, minScore }`
- `tools`

常用方法：`addMessage`、`getMessages`、`getTokenUsage`、`chat`、`chatStream`、`save`、`reset`、`flush`（等待后台记忆提取完成，`save` 前自动调用）、`pendingMemoryTasks`（挂起提取任务数）。`chat/chatStream` 支持 `signal` 取消。

flush 对本节点后台提取的异常和失败 HaiResult 返回 MEMORY_EXTRACT_FAILED（含失败数量）；pendingMemoryTasks=0 不保证成功。同轮并发观察者共享结果，返回即确认本批报告，后续只观察新任务，不自动重放未知副作用。save、reset({ waitForMemoryTasks: true })、consolidate 会传播失败并停止后续写入，调用方必须检查 success。

## File / Rerank / Reasoning

- `ai.file.parse({ content, filename, options })`：text/html/pdf/docx/ocr 解析。
- `ai.file.parseText(content, filename?)`：直接返回文本。
- `ai.rerank.rerank(request)` / `rerankTexts(query, texts, topN?)`：相关性重排。
- `ai.reasoning.run(query, options?)` / `runStream(query, options?)`：`react`、`cot`、`plan-execute`。

## Audio

IndexTTS 的 Node HTTP 请求使用同版本 Undici fetch/Agent/FormData；模型 timeout 覆盖响应头与正文等待，主动取消为 AUDIO_CANCELLED、超时为 AUDIO_TIMEOUT，结束后释放独立连接。完整调用返回 HaiResult，流式降级保留迭代抛错。

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `transcribe(request)` | `Promise<HaiResult<TranscriptionResult>>` | 完整音频 → 完整文本 |
| `transcribeStream(request)` | `AsyncIterable<TranscriptionEvent>` | 完整音频或 `AudioInputStream` → `speech_started` / `{type:'transcript',text,final}` / `speech_stopped`（VAD 平台产出语音起止事件） |
| `synthesize(request)` | `Promise<HaiResult<SynthesisResult>>` | 完整文本 → 完整音频 |
| `synthesizeStream(request)` | `AsyncIterable<SynthesisEvent>` | 带 ID 文本段 → `segment_started → audio* → segment_done`，事件均可关联 `segmentId`；`segment_started` 携带服务端解析后的真实 `format` / `sampleRate?` / `channels?` |
| `getCapabilities({ operation, model? })` | `HaiResult<AudioModelCapabilities>` | 按模型与操作查询 `transcribe` / `synthesize` 能力分支，并拒绝操作不匹配模型 |

- 音频类型：`AudioContent { data: Uint8Array, format: 'pcm16'|'wav'|'mp3'|'opus', sampleRate?, channels? }`；`pcm16` 等裸音频必须传 `sampleRate`。
- 播放负载：`serializePlayableAudio(audio)` 返回 `HaiResult<PlayableAudio>`；`PlayableAudio` 为 `{ audioBase64, format: 'wav'|'mp3', mimeType }`。该函数将 `pcm16` 封装为 WAV，透传 WAV/MP3；`opus` 不支持转换。
- 请求可选：识别 `contextHints?: string[]`（热词/提示）；合成 `instruction?: string`（自然语言风格指令）；均支持 `signal: AbortSignal`。
- 模型配置：`audio.models: [{ id, provider: 'openai'|'mimo'|'qwen'|'doubao', model, operations, apiKey?, baseUrl?, appKey?, accessKey?, resourceId?, workspaceId? }]`；`operations` 必须明确为识别、合成或两者。确认 LLM 与语音模型共用凭据时，可设置 `audio.inheritLlmApiKey: true`，默认关闭。
- 凭据优先级：模型条目 `apiKey` → 显式启用继承后的 LLM `apiKey` → `HAI_AI_AUDIO_<PROVIDER>_APIKEY` 或 `OPENAI_API_KEY` / `MIMO_API_KEY` / `DASHSCOPE_API_KEY` / `VOLC_API_KEY`（豆包旧版控制台额外 `VOLC_APP_KEY` / `VOLC_ACCESS_KEY`）。可选密钥中的 YAML `null`、空字符串和纯空白字符串都会规范化为未配置。
- 资源上限：`audio.maxAudioBytes`（默 10 MiB）、`audio.maxStreamDurationMs`（默 5 分钟）；所有请求支持 `signal: AbortSignal` 打断。
- Provider 为内部实现，不从根入口导出；不支持的输入方式招 `AUDIO_UNSUPPORTED_INPUT`，不伪装实时。取消 → `AUDIO_CANCELLED`，超时 → `AUDIO_TIMEOUT`，连接失败或 `end` 前异常断连 → `AUDIO_CONNECTION_FAILED`。浏览器客户端（`@h-ai/ai/client`）严格区分正常结束 / 取消 / 异常断连：取消招 `AUDIO_CANCELLED`、`end` 前断连招 `AUDIO_CONNECTION_FAILED`、服务端 error 帧保留其领域错误码，`synthesize` 不把未完成的部分音频当作成功返回。

## Image

| 方法 | 返回 | 说明 |
| --- | --- | --- |
| `generate(request)` | `Promise<HaiResult<GenerateImageResult>>` | 文本提示词 → 一张或多张已下载的图片字节 |

- 请求：`GenerateImageRequest { prompt, model?, size?: { width, height }, referenceImages?: ReferenceImage[], signal? }`；`ReferenceImage` 为 `{ data: Uint8Array, mimeType: string }`。
- 返回：`GenerateImageResult { images: Array<{ data: Uint8Array, mimeType, width?, height? }> }`。
- 模型配置：`image.models: [{ id, provider: 'openai'|'google'|'qwen'|'seedream'|'pollinations', model, apiKey?, baseUrl?, workspaceId?, timeout? }]`；`image.generateModel` 选择默认模型别名。
- `image.maxImageBytes` 默认20971520、最大104857600，限制单张输出/参考图；`image.maxImages` 默认4、范围1–16。JSON正文最多max(64KiB,两倍图片上限)，错误正文64KiB，实际解压后字节受限，URL串行下载。环境入口HAI_AI_IMAGE_MAXIMAGEBYTES / HAI_AI_IMAGE_MAXIMAGES。
- 临时URL仅允许无凭证、默认443公网HTTPS，实际socket DNS候选全部校验并固定使用，私网/回环/链路本地/映射/转换/保留地址拒绝；最多3次重定向且每跳校验，不传认证头/Cookie。baseUrl为可信运维配置，厂商API不跟随重定向。模型timeout默认120000ms覆盖整次生成/下载，signal可取消。
- 输出PNG/JPEG/GIF/WebP须MIME与魔数相符，SVG拒绝；宽高仅为请求/厂商提示，不做完整解码。地址拒绝/超限/超时分别为IMAGE_DOWNLOAD_BLOCKED/IMAGE_TOO_LARGE/IMAGE_TIMEOUT，调用方检查success后再使用。
- 凭据优先使用模型项 `apiKey`，否则回退 `HAI_AI_IMAGE_<PROVIDER>_APIKEY` 或厂商标准环境变量。
- 厂商差异由内部 Provider 消化：OpenAI/Pollinations 的 multipart edits、Google `inlineData`、Qwen/Seedream 的 Data URL 参考图，以及各家 Base64、临时 URL 或二进制响应均归一为图片字节。
- Google 只接受其文档列出的宽高比；Qwen/Seedream 还可能对总像素与边长有限制。业务应提供厂商支持的尺寸预设，不要假设任意像素组合都可用。

## LLM 结构化输出

`ai.llm.generateObject({ schema, messages, model?, systemPrompt?, maxRepairs?, signal? })`：按 Zod schema 约束模型输出（json_schema）并解析为对象，解析/校验失败自动带错误提示重试（默 1 次）。适用于“是否为问题/意图分类/置信度”等需要稳定结构结果的场景，避免手写 `JSON.parse`。

## A2A

| 方法 | 说明 |
| --- | --- |
| `registerExecutor(executor)` | 注册 Agent executor |
| `getAgentCard()` | 获取 Agent Card |
| `handleRequest(body, context?)` | 处理 A2A JSON-RPC 请求 |
| `listMessages(filter)` | 查询 A2A 消息记录 |
| `callRemoteAgent(remoteUrl, message, options?)` | 调用远端 Agent；只依赖 `ai.init()`，不要求本地 executor |

A2A 服务端 SDK handler 在注册 executor 时延迟创建；远端客户端调用不依赖服务端配置。getAgentCard 返回完整 A2A 0.3 Card（MIME、streaming、安全方案）；Kit 默认发现路径为 `/.well-known/agent-card.json`。handleRequest 在未初始化时也返回 JSON-RPC 错误；可信 context.agentId 映射 SDK user.userName。Kit 配置 authenticate 后空身份返回 401；不声明 push notifications、gRPC、REST 支持。

A2A journal、context 归属和消息表在 `ai.init()` 阶段初始化；可信 agentId 是身份来源，匿名共用主体，生产须认证。Task 与事件一次 CAS 发布，支持任意节点订阅、真实 owner 取消和 input-required 跨节点续接。同 message ID/内容重试复用同一轮，内容改变或旧轮重试拒绝；取消未确认时同一 Task 拒绝新消息，防止迟到回调影响新一代；失租、绝对期限或 working 状态退出标记 failed/interrupted 与 metadata.haiExecution.requiresEffectReconciliation=true，不自动重放未知副作用。a2a.execution 默认 leaseMs=30000、pollIntervalMs=250、maxExecutionMs=300000；单消息1MiB、journal8MiB、每轮2048事件、每Task256回执、32个reference tasks。listMessages 是服务端管理接口，callerId/contextId/since 不能替代调用授权。外部工具须使用稳定业务幂等键并核对未知结果，SDK异常不向用户暴露驱动凭据。

`callRemoteAgent` 只接受 HTTP(S) 且拒绝 URL 内嵌凭据。该校验不是完整 SSRF 防护：remote URL 若来自外部输入，应用必须先按 origin 白名单过滤，并通过出口代理限制 DNS 重绑定、重定向到私网与云元数据地址。

## 错误码速查

| 分段 | 代表错误 |
| --- | --- |
| `hai:ai:000` | `INTERNAL_ERROR` |
| `hai:ai:010-012` | 初始化：`NOT_INITIALIZED` / `CONFIGURATION_ERROR` / `INIT_IN_PROGRESS` |
| `hai:ai:020-033` | Rerank / File |
| `hai:ai:050-059` | Audio：`AUDIO_INVALID_REQUEST` / `MODEL_NOT_FOUND` / `PROVIDER_NOT_FOUND` / `UNSUPPORTED_INPUT` / `UPSTREAM_ERROR` / `PROTOCOL_ERROR` / `CONNECTION_FAILED` / `TIMEOUT` / `INPUT_TOO_LARGE` / `CANCELLED` |
| `hai:ai:060-068` | Image：`IMAGE_INVALID_REQUEST` / `MODEL_NOT_FOUND` / `PROVIDER_NOT_FOUND` / `UPSTREAM_ERROR` / `PROTOCOL_ERROR` / `CANCELLED` / `DOWNLOAD_BLOCKED` / `TOO_LARGE` / `TIMEOUT` |
| `hai:ai:100-107` | LLM / 历史记录 |
| `hai:ai:200-204` | MCP |
| `hai:ai:300-302` | Embedding |
| `hai:ai:400-405` | Tool（含重复注册、授权拒绝） |
| `hai:ai:500-502` | Reasoning |
| `hai:ai:600-701` | Retrieval / RAG |
| `hai:ai:800-805` | Knowledge |
| `hai:ai:900-906` | Memory |
| `hai:ai:950-971` | Context / Store / Session |
| `hai:ai:980-984` | A2A |

## 浏览器端

浏览器端通过 `@h-ai/api-client` 或自定义 endpoint 访问服务端 AI 能力。使用 serv 默认 AI feature 时，objectId 从可信认证 session 绑定，客户端可省略；其它主体默认拒绝，跨用户管理必须由服务端显式 authorizeObject。聊天/消息写入、历史、会话和记忆均按该主体隔离，sessionId 是主体内的会话标识。浏览器入口不提供 Node 端 `ai` 实例或 `ai.stream`；流式与 A2A 使用应用实际声明的独立契约。

默认 `chats.createCompletion`（`POST /ai/chats/completions`）只返回非流式 `HaiResult<AiChatCompletionData>`，未声明字段（含 `stream: true/false`）在 HTTP 边界拒绝。function `tools` 保留完整嵌套 JSON Schema 与 `strict`，`tool_choice` 支持 auto/none/required/本次定义的函数名。消息按角色校验并保留 name、用户图片、助手 tool_calls；工具结果必须带非空 tool_call_id。应用自行授权执行工具后，将助手调用与对应结果回传。需要流式时定义与 `ai.llm.chatStream()` 匹配的独立 endpoint，不把 stream 填入默认补全请求。

```ts
const response = await apiClient.ai.chats.createCompletion({ messages })
const memories = await apiClient.ai.memories.recall({ query: '偏好', topK: 5 })
```

语音：`@h-ai/ai/client` 的 `createAudioClient({ url, getTicket })` 提供 `client.transcribe/transcribeStream/synthesize/synthesizeStream`。`getTicket(request)` 接收本次操作摘要（`operation` / `model`），应通过已登录 HTTP 请求签发与本次操作严格绑定的短期、一次性 Audio ticket（推荐 `iam.ticket.issue({ grant })`）；IAM access token 不进入 WebSocket URL。

Knowledge 分块直接遵循 datapipe 的单位与硬上限：word 为词，其他为 Unicode code point，保留清洗后文本的分隔符；AI 默认 chunkOptions 仍为 markdown/maxSize=1500/overlap=200，覆盖较小 maxSize 时须同时保证继承的 overlap 小于它。分块配置/表达式失败返回 KNOWLEDGE_INGEST_FAILED，embedding 与版本发布均不会执行，不退回整篇文本绕过预算。

JSDoc 公开示例使用真实包入口，内部工厂示例标记“模块内部上下文”；应用使用所安装版本的公开类型，并通过本应用已有的类型检查验证使用示例。调用示例须保留 HaiResult 成功检查、受信主体范围、共享 CAS 版本与存储端期限，不将后台 flush 失败或未知外部效果当作成功重试。

SSE：Node 的 ai.stream 解码器支持跨片 LF/CRLF/CR、多行 data，只去冒号后一个空格；无 data 块不派发，空或缺省事件名按 message，id/retry 继承最近有效值，reset 清空连接状态，EOF 不补空行。encodeSSE 允许合法 data 换行并规范化，id/event 换行、id 的 NUL、非负安全整数以外的 retry 显式抛 TypeError/RangeError。真实 Anthropic/MiMo 使用同一内部 UTF-8 reader，退出/取消/协议结束/错误均取消正文并释放 reader；无自动重连/副作用重放。Anthropic 总 timeout 覆盖正文，MiMo 保留每请求独立 timeout。
