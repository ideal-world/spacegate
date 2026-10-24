---
name: hai-scheduler
description: "注册或排查 cron/API/JS/Hook 任务、手工触发、执行日志、重试取消与多节点调度。"
---

# hai-scheduler

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/scheduler 注册 cron/API/JS/Hook 任务、手工触发并查询执行日志 |
| 适用场景 | 服务端任务调度、持久化和手工触发 |
| 输入 | SchedulerConfigInput、任务定义、已初始化依赖与 hooks |
| 输出 | 任务管理、触发和日志查询的 HaiResult |
| 限制 | 仅服务端；vm 不是安全沙箱，JS 任务只执行可信代码。共享RelDB强制持久journal/租约；unknown保留槽直至可信人工裁定。 |

## 使用路径

1. 始终先初始化同一共享 reldb；enableDb 只能 true，跨主机生产使用 PostgreSQL/MySQL，不需要缓存。检查 scheduler.init 的 HaiResult。
2. 动态共享任务通过 register 管理，不与 init.tasks 重复声明；读取失败、禁用、删除或改期时不得继续旧任务。
3. 根据可信逻辑选择 API/JS/Hook。Worker/vm 不是安全沙箱；Hook 监听 signal，业务以 occurrenceId 原子持久去重，只有实际满足该约束才设 idempotent:true；API 自动覆盖幂等头。
4. 明确 timeout、响应大小、重试与执行状态；关闭时 await scheduler.close，再关闭数据库；unknown须先确认旧runner终止并核对副作用，按版本人工resolveUnknown，禁止直接重排。
5. 验证手动/定时、跨周期租约、手动/cron竞争及暂停接管、失败重试、取消与定义更新；共享租约必须在 SQL 内用存储时钟原子检查，cron 分钟身份也取 DB 时间且默认 UTC；补上节点 ±2h、不同 TZ 与真实行锁跨期限的多进程用例。
6. 检查业务 status 与独立 persistence.status；pending 通过 execution.listLogFailures/repairLog 从已发布 journal 补写，不能重新触发业务。失租 unconfirmed 不能作为成功证据；验证丢回执和双节点并发补写。

7. 核对 JSDoc 示例的真实入口与返回值：HaiResult 先分支，close 直接 await；标注“模块内部上下文”的仓库/runner 不属于根 API。示例编译与共享 DB/进程行为验证分别记录。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [运行环境](reference.md#运行环境)
- [依赖](reference.md#依赖)
- [使用步骤](reference.md#使用步骤)
- [核心 API](reference.md#核心-api)
- [常见模式](reference.md#常见模式)
- [错误码 — `HaiSchedulerError`](reference.md#错误码--haischedulererror)
