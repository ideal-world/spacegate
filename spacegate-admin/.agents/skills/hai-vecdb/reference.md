# hai-vecdb — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/vecdb` 提供统一的向量数据库操作接口，支持 LanceDB、pgvector、Qdrant、Chroma，包含集合管理和向量 CRUD/搜索。

## 运行环境

> ⚠️ **服务端模块（Node.js only）。** 浏览器端不直接操作向量数据库，通过 `@h-ai/ai` 的 API 端点间接使用（如 RAG 检索、知识库查询）。

## 适用场景

- 新增或修改向量数据库访问逻辑（集合管理/向量插入/搜索）
- 接入不同向量数据库后端（LanceDB / pgvector / Qdrant / Chroma）
- 基于 `HaiVecdbError` 做错误分支处理
- 构建 RAG、语义搜索、知识库等 AI 场景

## 使用步骤

### 1. 配置

```yaml
# config/_vecdb.yml

# LanceDB（默认，嵌入式本地存储）
type: lancedb
path: ./data/vecdb

# pgvector
# type: pgvector
# url: postgres://user:pass@localhost:5432/mydb
# indexType: hnsw
# tablePrefix: vec_

# Qdrant（服务端 1.10.0+，使用 Query API）
# type: qdrant
# url: http://localhost:6333
# apiKey: ''

# Chroma（嵌入式：提供 path 且无 url 时自动拉起本地 `chroma run` 服务并持久化）
# type: chroma
# path: ./data/chroma
# Chroma（直连已有服务，不拉起进程）
# type: chroma
# url: http://localhost:8000
# apiKey: ''
# serverCommand: chroma      # 嵌入式服务端命令，默认 chroma；需独立安装服务端 CLI
# startupTimeout: 30000      # 嵌入式服务就绪等待超时（毫秒）
operationLog:
  read: false # collection.exists/info/list, vector.search/count
  write: false # collection.create/drop, vector.insert/upsert/delete
  maxLength: 1000
  level: debug # info | debug | trace
```

约定环境变量优先于 YAML，例如 `HAI_VECDB_URL`、
`HAI_VECDB_APIKEY`、`HAI_VECDB_OPERATIONLOG_LEVEL`。

`operationLog` 在集合/向量操作进入真实 Provider 前输出日志，不要在 `vecdb-main.ts` 或调用方重复包装。`maxLength` 用于截断序列化后的向量、文档和过滤条件，`level` 默认 `debug`。

### 2. 初始化与关闭

```typescript
import { core } from '@h-ai/core'
import { vecdb } from '@h-ai/vecdb'

await vecdb.init(core.config.get('vecdb'))
// ... 使用向量数据库
const closeResult = await vecdb.close()
if (!closeResult.success) {
  throw new Error(closeResult.error.message)
}
```

> `vecdb.config` 返回的是**脱敏配置快照**；连接字符串中的用户名/密码、独立 `password` 字段和 `apiKey` 会被替换为 `[REDACTED]`。

### 3. 选择操作接口

| 接口       | 用途                         | 入口               |
| ---------- | ---------------------------- | ------------------ |
| collection | 集合创建/删除/查询/判断存在  | `vecdb.collection` |
| vector     | 向量插入/更新/删除/搜索/计数 | `vecdb.vector`     |

## 核心 API

### 集合操作 — `vecdb.collection`

| 方法     | 签名                                  | 说明             |
| -------- | ------------------------------------- | ---------------- |
| `create` | `(name, options) => HaiResult<void>`  | 创建集合         |
| `drop`   | `(name) => HaiResult<void>`           | 删除集合         |
| `exists` | `(name) => HaiResult<boolean>`        | 判断集合是否存在 |
| `info`   | `(name) => HaiResult<CollectionInfo>` | 获取集合信息     |
| `list`   | `() => HaiResult<string[]>`           | 列出所有集合     |

> 所有方法均返回 `Promise<HaiResult<T>>`，上表省略异步与错误类型。

```typescript
// 创建集合（指定维度和度量）
await vecdb.collection.create('docs', { dimension: 1536, metric: 'cosine' })

// 查询集合信息
const info = await vecdb.collection.info('docs')
if (info.success) {
  // info.data => { name, dimension, metric, count }
}

// 列出所有集合
const list = await vecdb.collection.list()
```

**CollectionCreateOptions**：

```typescript
interface CollectionCreateOptions {
  dimension: number // 向量维度（必填）
  metric?: 'cosine' | 'euclidean' | 'dot' // 距离度量（默认 cosine）
}
```

LanceDB 将向量维度存于 Arrow 固定长度 vector 字段，将集合 metric 存于同一 schema 的 metadata；空表也可正确恢复。`collection.create` 中的 metric 优先于连接配置，连接 metric 仅是新集合的默认值。重启或更换默认值不会改变已有集合。每次操作读取持久 schema，并检查其它连接的提交；缺少有效集合定义时返回 `CONFIG_ERROR`，不探测假向量或猜测 metric。旧表/外部表如缺少该定义，需要重建集合，不提供兼容推断。

LanceDB 是嵌入式本地文件后端。只有访问相同持久目录、且底层文件系统支持 LanceDB 并发提交的进程才能共享数据；不同节点各自的同名本地路径互不共享。分布式服务优先选择 pgvector、Qdrant 或 Chroma 服务端。AI Knowledge 的 reldb 与 vecdb 都须持久化并指向同一业务数据集，替换 embedding 维度时须重建匹配的集合。

搜索始终使用集合 metric，`euclidean` 映射 LanceDB 的 `l2`。`score` 延续公开 API 的 0–1、越大越相似约定：cosine 为 `clamp(1 - distance / 2, 0, 1)`；euclidean 为 `1 / (1 + squaredL2)`；dot 为 `sigmoid(dotProduct)`，用于保持内积排序而非概率。`minScore` 在该转换后过滤；不同 metric/后端的阈值不应视为通用校准值。

LanceDB 原子写入的验证基线为 `@lancedb/lancedb 0.40.0 / Lance 13`。新集合在初始 Arrow schema 声明 id 为不可空主键；insert/upsert 均使用原生 merge，以 id 参与严格键冲突检测。依据：[Lance 13 table format](https://github.com/lance-format/lance/blob/v13.0.0/docs/src/format/table/index.md#unenforced-primary-key)、[Lance 13 transaction](https://github.com/lance-format/lance/blob/v13.0.0/protos/transaction.proto)、[Node binding 0.40.0](https://github.com/lancedb/lancedb/blob/v0.40.0/nodejs/lancedb/table.ts)。

LanceDB 的 `insert` 仅插入新 ID，已有 ID 保留不覆盖；`upsert` 新增或整体替换。同批重复 ID、缺失 merge 能力/持久主键均返回 `CONFIG_ERROR`，不执行写入。旧表需重建，不自动迁移或清空。每个 merge 独立原子提交；并发胜者由提交顺序决定，不提供多个 merge 或 reldb/vecdb 间的共同事务。框架不会回退到 delete/add；写入结果不明时按 ID 核对。直接使用原生 add 的外部写入会绕过该保护，不属于本接口保证。

LanceDB metadata 使用原生 JSON 字段，仍可保存嵌套 JSON 对象，搜索结果还原为对象；insert/upsert 明确使用 Arrow JSON schema 完成 JSONB 编码。旧 Utf8 metadata 表写入或过滤返回 CONFIG_ERROR，需要重建，不自动迁移。依据：[Lance 13 JSON UDF](https://github.com/lance-format/lance/blob/v13.0.0/rust/lance-datafusion/src/udf/json.rs)、[Node 0.40.0 JSON field](https://github.com/lancedb/lancedb/blob/v0.40.0/nodejs/lancedb/arrow.ts)。

LanceDB filter 只支持顶层字面量键的 AND 精确匹配，值为 string/有限 number/boolean/null；例如 `{ tenant: 'acme', page: 1, active: true, removedAt: null }`。数字/布尔不匹配同形字符串；null 只匹配显式 JSON null，不匹配缺失键。引号、反斜线、百分号、下划线均为字面量，点/括号不解释为嵌套路径。空对象不筛选；对象、数组、undefined、非有限数值返回 CONFIG_ERROR，不能忽略条件重试。过滤在向量 topK 前执行，随后应用 minScore；其它 Provider 沿用原生约束，不保证同样的 null、复杂值和路径语义。

### 向量操作 — `vecdb.vector`

| 方法     | 签名                                                          | 说明          |
| -------- | ------------------------------------------------------------- | ------------- |
| `insert` | `(collection, documents) => HaiResult<void>`                  | 批量插入      |
| `upsert` | `(collection, documents) => HaiResult<void>`                  | 批量更新/插入 |
| `delete` | `(collection, ids) => HaiResult<void>`                        | 按 ID 删除    |
| `search` | `(collection, vector, options?) => HaiResult<SearchResult[]>` | 向量搜索      |
| `count`  | `(collection) => HaiResult<number>`                           | 文档计数      |

> 所有方法均返回 `Promise<HaiResult<T>>`，上表省略异步与错误类型。

```typescript
// 插入向量文档
await vecdb.vector.insert('docs', [
  { id: 'doc-1', vector: embedding, content: '文档内容', metadata: { source: 'wiki' } },
  { id: 'doc-2', vector: embedding2, content: '另一段内容' },
])

// 向量搜索（返回相似度排序结果）
const result = await vecdb.vector.search('docs', queryVector, {
  topK: 10,
  minScore: 0.7,
  filter: { source: 'wiki' },
})
if (result.success) {
  for (const item of result.data) {
    // item => { id, score, content, metadata }
  }
}

// 更新已有文档（按 id 匹配）
await vecdb.vector.upsert('docs', [
  { id: 'doc-1', vector: newEmbedding, content: '更新后的内容' },
])

// 删除
await vecdb.vector.delete('docs', ['doc-1', 'doc-2'])
```

> 空批量 `insert/upsert/delete` 会被视为 no-op；写入与搜索会校验向量维度，不匹配时返回 `HaiVecdbError.DIMENSION_MISMATCH`。Qdrant `collection.exists()` 仅在 404 时返回 `false`，网络异常会返回查询错误而不是误判不存在。

Qdrant `insert/upsert/delete` 等待服务端应用完成（wait:true）才返回成功，`vector.count` 请求精确计数。多次操作之间没有事务；Knowledge 在此基础上使用共享版本清单发布与补偿。

> Chroma 在 Node 端只有 HTTP 客户端（无进程内嵌入式）。嵌入式模式（提供 `path` 且无 `url`）下 `vecdb.init` 通过 `serverCommand`（默认 `chroma`）拉起本地服务，`vecdb.close` 时关闭进程；npm `chromadb` 提供客户端，服务端 CLI 需另行安装。服务命令不可用或就绪超时时 `init` 返回 `HaiVecdbError.CONNECTION_FAILED`。

Chroma 直连 `url` 仅支持 HTTP(S) 根端点，优先于 `host/port`；URL 不得带凭据、路径前缀、query 或 fragment。无 `url` 时 `host/port` 使用 HTTP。可选 `apiKey` 以 `Authorization: Bearer <apiKey>` 发送给采用此协议的自托管服务/网关；远程传输应使用 HTTPS。空串表示未配置，其余值只允许无空白的可打印 ASCII。本接口使用默认 tenant/database，不提供 Chroma Cloud 契约。初始化在心跳后验证数据端点权限，认证/权限失败返回 `CONNECTION_FAILED`，必须先检查 `success` 再执行业务。SDK 参数依据：[chromadb 3.5.0 官方客户端](https://docs.trychroma.com/reference/typescript/client)。

直连初始化总截止为 5 秒，本地服务拉起使用 `startupTimeout`；超时中止候选 HTTP 请求。成功清除初始化 timer，不限制后续业务请求。

Qdrant 使用 `@qdrant/js-client-rest 1.18.0`，仅支持未命名向量字段的 cosine/euclidean/dot；外部 named-vector/Manhattan 集合的 info/写入/搜索返回 CONFIG_ERROR，不猜测维度或 metric。Chroma 使用 `chromadb 3.5.0` 的真实类型及 metadata/filter 校验，非法输入返回操作错误，不丢弃过滤；content 可省略。各后端的 null 和复杂元数据语义不承诺统一。

**VectorDocument**：

```typescript
interface VectorDocument {
  id: string // 文档唯一标识
  vector: number[] // 向量数据
  content?: string // 文本内容（可选）
  metadata?: Record<string, unknown> // 元数据（可选）
}
```

**VectorSearchOptions**：

```typescript
interface VectorSearchOptions {
  topK?: number // 返回数量（默认 10）
  filter?: Record<string, unknown> // Provider 原生约束；LanceDB 仅支持上述顶层标量 AND 匹配
  minScore?: number // 最低相似度阈值（0-1）
}
```

## 错误码 — `HaiVecdbError`

| 错误码                                    | code            | 说明               |
| ----------------------------------------- | --------------- | ------------------ |
| `HaiVecdbError.CONNECTION_FAILED`         | `hai:vecdb:001` | 连接失败           |
| `HaiVecdbError.QUERY_FAILED`              | `hai:vecdb:002` | 查询失败           |
| `HaiVecdbError.COLLECTION_NOT_FOUND`      | `hai:vecdb:003` | 集合不存在         |
| `HaiVecdbError.COLLECTION_ALREADY_EXISTS` | `hai:vecdb:004` | 集合已存在         |
| `HaiVecdbError.DIMENSION_MISMATCH`        | `hai:vecdb:005` | 向量维度不匹配     |
| `HaiVecdbError.INSERT_FAILED`             | `hai:vecdb:006` | 插入失败           |
| `HaiVecdbError.DELETE_FAILED`             | `hai:vecdb:007` | 删除失败           |
| `HaiVecdbError.UPDATE_FAILED`             | `hai:vecdb:008` | 更新失败           |
| `HaiVecdbError.INDEX_BUILD_FAILED`        | `hai:vecdb:009` | 索引构建失败       |
| `HaiVecdbError.NOT_INITIALIZED`           | `hai:vecdb:010` | 未初始化           |
| `HaiVecdbError.CONFIG_ERROR`              | `hai:vecdb:011` | 配置错误           |
| `HaiVecdbError.UNSUPPORTED_TYPE`          | `hai:vecdb:012` | 不支持的数据库类型 |
| `HaiVecdbError.DRIVER_NOT_FOUND`          | `hai:vecdb:013` | 驱动未安装         |
| `HaiVecdbError.SERIALIZATION_FAILED`      | `hai:vecdb:014` | 序列化失败         |

## 常见模式

### RAG 检索

```typescript
import { ai } from '@h-ai/ai'
import { vecdb } from '@h-ai/vecdb'

// 将查询文本转为向量
const embedResult = await ai.embedding.embedText(query)
if (!embedResult.success)
  return embedResult

// 检索最相似的文档
const searchResult = await vecdb.vector.search('knowledge', embedResult.data, {
  topK: 5,
  minScore: 0.7,
})
if (!searchResult.success)
  return searchResult

// 拼接上下文，送入 LLM
const context = searchResult.data.map(r => r.content).join('\n\n')
const chatResult = await ai.llm.chat({
  messages: [
    { role: 'system', content: `参考以下资料回答用户问题：\n${context}` },
    { role: 'user', content: query },
  ],
})
```

### 文档入库

```typescript
import { ai } from '@h-ai/ai'
import { datapipe } from '@h-ai/datapipe'
import { vecdb } from '@h-ai/vecdb'

// 清洗 + 分块
const pipeline = await datapipe.pipeline()
  .clean({ removeHtml: true })
  .chunk({ mode: 'markdown', maxSize: 1000, overlap: 100 })
  .run(rawText)

if (!pipeline.success)
  return pipeline

// 批量生成嵌入
const embeddings = await ai.embedding.embedBatch(
  pipeline.data.chunks.map(c => c.content),
)
if (!embeddings.success)
  return embeddings

// 批量入库
const docs = pipeline.data.chunks.map((chunk, i) => ({
  id: `doc-${i}`,
  vector: embeddings.data[i],
  content: chunk.content,
  metadata: chunk.metadata,
}))
await vecdb.vector.insert('knowledge', docs)
```

### 错误分支处理

```typescript
import { HaiVecdbError, vecdb } from '@h-ai/vecdb'

const result = await vecdb.collection.create('docs', { dimension: 1536 })
if (!result.success) {
  switch (result.error.code) {
    case HaiVecdbError.COLLECTION_ALREADY_EXISTS.code:
      // 集合已存在，可跳过
      break
    case HaiVecdbError.NOT_INITIALIZED.code:
      // 未初始化，需先 vecdb.init()
      break
    default:
      // 意外错误
      break
  }
}
```

## 相关 Skills

- `hai-ai`：LLM 与 Embedding 能力
- `hai-datapipe`：文本清洗与分块
- `hai-reldb`：关系型数据库（知识库需要同时使用 reldb 存储结构化数据）
- `hai-core`：配置管理、HaiResult 模型

## 生产共享状态

生产使用 Qdrant、pgvector 或远端 Chroma；LanceDB 和嵌入式 Chroma 只用于开发/明确测试。 NODE_ENV 或 HAI_CORE_ENV 为 production 时在初始化成功前检查，只有 HAI_E2E=1/HAI_E2E_MOCK=1 明确测试豁免。不要将租约当作最终写入条件，外部未知效果须按稳定业务键核对。

## 关闭与失败恢复

vecdb.close() 失败后不要把资源视为已释放；error.ext.cleanupPending 表示同一模块仍保有待清理资源。旧操作引用也会返回 NOT_INITIALIZED；重试 close 或 init 的前置清理，不创建并行替代连接。内嵌 Chroma 只有直接子进程 exit 确认后关闭成功，终止信号回执不是退出证明。
