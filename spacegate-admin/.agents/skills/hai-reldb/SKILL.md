---
name: hai-reldb
description: "使用 @h-ai/reldb 访问 SQLite/PostgreSQL/MySQL，编写 SQL、CRUD、DDL、事务和分页。"
---

# hai-reldb

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/reldb 访问 SQLite/PostgreSQL/MySQL，编写 SQL、CRUD、DDL、事务和分页 |
| 适用场景 | 关系数据读写、仓库封装或事务一致性 |
| 输入 | ReldbConfigInput、参数化 SQL、CRUD 配置、事务句柄 |
| 输出 | DB 操作为 HaiResult；crud.table 和分页/JSON 工具按签名返回 |
| 限制 | 运行时仅服务端，纯 schemas 入口可用于浏览器；tx.wrap 只在回调抛错时回滚，返回失败 HaiResult 不会触发回滚。动态值参数化，标识符用白名单。 |

## 使用路径

- `_db.yml` 对应 core.config 的 db；先校验 ReldbConfigSchema，再 init 并检查 success。仅做配置校验可从 `@h-ai/reldb/schemas` 导入，纯入口只依赖 Zod，不加载数据库实例/驱动，可用于 CLI 和浏览器；运行时数据库操作仍仅服务端。
- `reldb.crud.table` / `tx.crud.table` 的 `dbType` 必填并与实际后端一致；`BaseReldbCrudRepository` 自动读取已初始化的配置。
- 事务每步检查结果，避免把失败当普通返回值提交；同一事务操作须共享 tx。
- SQL 值参数化；where/orderBy/json 列名不是任意用户输入。分页使用 PaginatedResult。
- MySQL 的 pool 仅 max/acquireTimeout/queueLimit；超时返回 TIMEOUT 并移除排队项，队列满返回 POOL_EXHAUSTED，迟到建连结算前仍占槽位。min/idleTimeout/timezone 不支持。URL 优先于分字段且禁止 query/fragment；TLS 仅 false 或严格对象，默认验证证书和 DNS 身份，验证开启的 IP 目标拒绝，cert/key 成对。详见 reference 的配置章节。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用步骤
- 核心 API
- 错误码 — `HaiReldbError`
- 常见模式
- 相关 Skills

JSON 对象合并使用 RFC 7396 补丁，必须将返回的 SQL/params 放在同一次 UPDATE 内；禁止先读对象再覆盖整个列来模拟跨节点原子合并。路径函数的后端差异和完整可类型检查示例见 [reference.md](reference.md)。
