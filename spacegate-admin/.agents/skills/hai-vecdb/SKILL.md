---
name: hai-vecdb
description: "使用 LanceDB/pgvector/Qdrant/Chroma 管理向量集合、写入和检索；排查维度、连接与过滤问题。"
---

# hai-vecdb

## 能力契约

| 项目     | 契约                                                                                                     |
| -------- | -------------------------------------------------------------------------------------------------------- |
| 能力     | 使用 @h-ai/vecdb 管理 LanceDB/pgvector/Qdrant/Chroma 集合与向量检索                                      |
| 适用场景 | 向量持久化、集合管理或相似度搜索                                                                         |
| 输入     | VecdbConfigInput、集合、固定维度向量、查询过滤条件                                                       |
| 输出     | 集合/向量操作的 HaiResult                                                                                |
| 限制     | 仅服务端；向量维度须匹配集合。Chroma 是 HTTP 服务，本地 path 模式需可用服务命令；init 失败不得当成空库。 |

## 使用路径

- `_vecdb.yml` 选择实际后端；init 后检查连接结果，再创建集合并写入相同维度向量。
- LanceDB 集合维度/metric 随 Arrow schema 持久化，连接 metric 仅是新集合默认值；本地路径不自动跨节点共享。分布式应用优先服务端后端，重启复用持久目录与匹配的 embedding 维度。
- LanceDB insert/upsert 均需持久非空 id 主键与原子 merge；insert 保留已有 ID，upsert 整行替换。同批重复 ID 或能力缺失返回 CONFIG_ERROR，无 delete/add 回退。原子性只覆盖每次 merge；检查错误后再继续业务写入。
- LanceDB 使用原生 JSON metadata；filter 仅支持顶层字面量键的 string/有限 number/boolean/null AND 匹配，null 不匹配缺失字段。对象/数组/undefined 与旧 Utf8 metadata 表返回 CONFIG_ERROR；过滤在 topK 前执行，错误不得当作空结果。其它后端的 null/复杂过滤能力须单独核对。
- Chroma 仅支持 HTTP(S) 根 url（优先于 host/port），不接受路径前缀/URL 凭据/query/fragment；apiKey 按自托管网关协议发送 Authorization: Bearer，不表示 Chroma Cloud 支持。初始化同时验证数据端点权限，认证失败不得当成空库。Qdrant 只有 404 代表集合不存在。
- Qdrant 仅支持未命名向量字段的三种公开 metric，外部 named-vector/Manhattan 集合返回 CONFIG_ERROR；Chroma 的 metadata/filter 由真实 SDK 校验，非法输入返回操作错误，不执行无过滤回退。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用步骤
- 核心 API
- 错误码 — `HaiVecdbError`
- 常见模式
- 相关 Skills
