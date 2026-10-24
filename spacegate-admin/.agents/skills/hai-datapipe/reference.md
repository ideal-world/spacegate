# hai-datapipe 参考

> `@h-ai/datapipe` 提供文本清洗（clean）、多模式分块（chunk）和可组合管线（pipeline），纯函数模块，无需初始化。

## 运行环境

> **服务端模块（Node.js only）。** 通常在 AI 知识库入库或文档预处理场景中使用，由 `@h-ai/ai` 内部调用或在服务端显式调用。

## 适用场景

- 文本清洗：移除 HTML 标签、URL、Email，标准化空白
- 文本分块：句子、段落、Markdown 标题、字数、字符、自定义分隔符
- RAG 入库前的文档预处理管线
- 管线模式组合多步清洗 + 分块 + 自定义转换

## 使用步骤

### 直接调用（无需 init）

```typescript
import { datapipe } from '@h-ai/datapipe'

// 清洗
const cleaned = datapipe.clean(htmlText, { removeHtml: true, removeUrls: true })

// 分块
if (!cleaned.success) return cleaned
const chunks = datapipe.chunk(cleaned.data, { mode: 'markdown', maxSize: 2000, overlap: 200 })
```

### 管线模式

```typescript
const result = await datapipe.pipeline()
  .clean({ removeHtml: true, removeUrls: true })
  .transform(text => text.toLowerCase())
  .chunk({ mode: 'paragraph', maxSize: 1000, overlap: 100 })
  .chunkTransform(chunks => chunks.filter(c => c.content.length > 50))
  .run(rawText)

if (result.success) {
  // result.data => { text: string, chunks: DataChunk[] }
}
```

## 核心 API

### 清洗 — `datapipe.clean`

```text
datapipe.clean(text: string, options?: CleanOptionsInput): HaiResult<string>
```

**CleanOptionsInput**：

| 字段                 | 类型               | 默认  | 说明                     |
| -------------------- | ------------------ | ----- | ------------------------ |
| `removeHtml`         | `boolean`          | true  | 移除 HTML 标签           |
| `removeUrls`         | `boolean`          | false | 移除 URL                 |
| `removeEmails`       | `boolean`          | false | 移除 Email 地址          |
| `normalizeWhitespace`| `boolean`          | true  | 多空格→单空格、去多余空行 |
| `trim`               | `boolean`          | true  | 去除首尾空白             |
| `customReplacements` | `{ pattern, replacement }[]` | — | 自定义正则替换规则 |

### 分块 — `datapipe.chunk`

```text
datapipe.chunk(text: string, options: ChunkOptionsInput): HaiResult<DataChunk[]>
```

**ChunkOptionsInput**：

| 字段               | 类型      | 默认 | 说明                           |
| ------------------ | --------- | ---- | ------------------------------ |
| `mode`             | `ChunkMode` | —   | 分块模式（必填）               |
| `maxSize`          | `number`  | 1500 | 分块最大大小                   |
| `overlap`          | `number`  | 0    | 重叠大小（上下文衔接）         |
| `separator`        | `string`  | —    | 自定义分隔符正则（mode=custom）|
| `markdownMinLevel` | `number`  | 2    | Markdown 最低标题级别（1-6）   |
| `markdownKeepTitle`| `boolean` | true | 分块中保留 Markdown 标题       |

**分块模式**：

| 模式        | 说明                        |
| ----------- | --------------------------- |
| `sentence`  | 按句子（中英文句号/问号/感叹号） |
| `paragraph` | 按段落（双换行）            |
| `markdown`  | 按 Markdown 标题层级        |
| `page`      | 按分页符（\f）              |
| `word`      | 按字数                      |
| `character` | 按字符数                    |
| `custom`    | 自定义正则分隔符            |

**DataChunk**：

```typescript
interface DataChunk {
  index: number                      // 分块索引（从 0 开始）
  content: string                    // 分块内容
  metadata?: Record<string, unknown> // 元数据（Markdown 模式含 title、level）
}
```

### 管线 — `datapipe.pipeline()`

```typescript
datapipe.pipeline()
  .clean(options)              // 添加清洗步骤
  .transform(fn)                // 添加文本转换步骤（同步/异步）
  .chunk(options)               // 添加分块步骤
  .chunkTransform(fn)           // 添加分块后处理步骤
  .run(text)                    // 执行管线 → Promise<HaiResult<PipelineResult>>
```

**PipelineResult**：

```typescript
interface PipelineResult {
  text: string        // 处理后的文本（分块前的最终文本）
  chunks: DataChunk[] // 分块列表（如果管线包含 chunk 步骤）
}
```

## 错误码 — `HaiDatapipeError`

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiDatapipeError.CLEAN_FAILED` | `hai:datapipe:001` | 清洗失败 |
| `HaiDatapipeError.CHUNK_FAILED` | `hai:datapipe:002` | 分块失败 |
| `HaiDatapipeError.TRANSFORM_FAILED` | `hai:datapipe:003` | 转换失败 |
| `HaiDatapipeError.PIPELINE_FAILED` | `hai:datapipe:004` | 管线执行失败 |
| `HaiDatapipeError.CONFIG_ERROR` | `hai:datapipe:005` | 配置错误 |
| `HaiDatapipeError.MISSING_SEPARATOR` | `hai:datapipe:006` | 自定义分隔符缺失 |

## 常见模式

### RAG 文档入库前处理

```typescript
import { datapipe } from '@h-ai/datapipe'
import { ai } from '@h-ai/ai'
import { vecdb } from '@h-ai/vecdb'

// 清洗 + Markdown 分块
const result = await datapipe.pipeline()
  .clean({ removeHtml: true, removeUrls: true })
  .chunk({ mode: 'markdown', maxSize: 1000, overlap: 100 })
  .run(rawDocument)

if (!result.success) return result

// 生成嵌入
const embeddings = await ai.embedding.embedBatch(
  result.data.chunks.map(c => c.content),
)
if (!embeddings.success) return embeddings

// 入库
const docs = result.data.chunks.map((chunk, i) => ({
  id: `chunk-${i}`,
  vector: embeddings.data[i],
  content: chunk.content,
  metadata: chunk.metadata,
}))
await vecdb.vector.insert('knowledge', docs)
```

### 多步清洗管线

```typescript
const result = await datapipe.pipeline()
  .clean({ removeHtml: true, removeEmails: true })
  .transform(text => text.replace(/\[.*?\]/g, ''))        // 移除 [注释]
  .transform(text => text.replace(/\(https?:.*?\)/g, '')) // 移除 Markdown 链接 URL
  .chunk({ mode: 'paragraph', maxSize: 800 })
  .chunkTransform(chunks => chunks.filter(c => c.content.length > 20)) // 过滤过短分块
  .run(rawText)
```

### 错误分支处理

```typescript
import { datapipe, HaiDatapipeError } from '@h-ai/datapipe'

const result = datapipe.chunk(text, { mode: 'custom' })
if (!result.success) {
  switch (result.error.code) {
    case HaiDatapipeError.MISSING_SEPARATOR.code:
      // mode='custom' 时需提供 separator
      break
    case HaiDatapipeError.CONFIG_ERROR.code:
      // 配置参数校验失败
      break
  }
}
```

## 相关 Skills

- `hai-vecdb`：向量数据库存储（先生成 embedding，再入库）
- `hai-ai`：LLM 与 Embedding 能力
- `hai-core`：HaiResult 模型

分块 `maxSize` 默认 1500，`overlap` 默认 0 且必须满足 `0 <= overlap < maxSize`。word 模式两者均按非空白词计量（连续中文可视为一个长词），其余模式均按 Unicode code point 计量，不是 UTF-16 length、字节或模型 token。上限包含重叠与分隔符；自然句/段/页/自定义边界只影响优先切点，超长段统一按 `maxSize-overlap` 滑窗。保留原始空白、标点、分页符及自定义分隔符，移除重叠后可还原全文；不再 split/join 或 trim 分块。Markdown section 之间不重叠，section 内使用相同硬上限，代码围栏内的标题保留为正文；仅 `markdownKeepTitle=false` 明确移除标题行。空/全空白的整体输入返回空数组。自定义正则须由可信配置提供，非法表达式返回 CHUNK_FAILED。
