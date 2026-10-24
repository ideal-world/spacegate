---
name: hai-datapipe
description: "使用 @h-ai/datapipe 清洗和分块文本、组合预处理管线；用于 RAG 入库前处理，不负责 embedding。"
---

# hai-datapipe

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/datapipe 清洗文本、分块并组合 RAG 入库管线 |
| 适用场景 | 文档进入检索/知识库前的文本预处理 |
| 输入 | 文本、清洗/分块选项、管线步骤 |
| 输出 | 清洗/分块的 HaiResult；管线工厂按签名使用 |
| 限制 | 纯函数，无 init/close；区分字符数和模型 token 数，分块结果不等于已生成 embedding。 |

## 使用路径

1. 直接使用 clean/chunk/pipeline，无 init/close；清洗或分块失败后不读 data。
2. 根据原文结构选择模式，明确 maxSize、overlap 与字符/词边界；这里不负责模型 token 预算。
3. 自定义正则仅来自可信配置。入库前先检查管线结果，再生成 embedding，并验证向量维度与集合匹配。
4. 验证空文本、非法配置、边界切分与自定义转换失败；保留文档内容及必要元数据。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [运行环境](reference.md#运行环境)
- [适用场景](reference.md#适用场景)
- [使用步骤](reference.md#使用步骤)
- [核心 API](reference.md#核心-api)
- [错误码 — `HaiDatapipeError`](reference.md#错误码--haidatapipeerror)
- [常见模式](reference.md#常见模式)
- [相关 Skills](reference.md#相关-skills)

分块 `maxSize` 默认 1500，`overlap` 默认 0 且必须满足 `0 <= overlap < maxSize`。word 模式两者均按非空白词计量（连续中文可视为一个长词），其余模式均按 Unicode code point 计量，不是 UTF-16 length、字节或模型 token。上限包含重叠与分隔符；自然句/段/页/自定义边界只影响优先切点，超长段统一按 `maxSize-overlap` 滑窗。保留原始空白、标点、分页符及自定义分隔符，移除重叠后可还原全文；不再 split/join 或 trim 分块。Markdown section 之间不重叠，section 内使用相同硬上限，代码围栏内的标题保留为正文；仅 `markdownKeepTitle=false` 明确移除标题行。空/全空白的整体输入返回空数组。自定义正则须由可信配置提供，非法表达式返回 CHUNK_FAILED。
