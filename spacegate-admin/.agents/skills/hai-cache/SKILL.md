---
name: hai-cache
description: "实现或排查 Memory/Redis 缓存、TTL、集合与分布式锁，处理过期、回源和并发边界。"
---

# hai-cache

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/cache 的 Memory/Redis 缓存、TTL、集合、排行榜和锁 |
| 适用场景 | 缓存读写、过期策略或共享资源互斥 |
| 输入 | CacheConfigInput、key/value、TTL；acquire 返回本次获锁唯一 owner |
| 输出 | 缓存和锁操作的 HaiResult，获锁 data 为唯一 owner 或 null |
| 限制 | 仅服务端；Memory 不跨节点。release/extend 携带本次 owner，避免锁过期后误操作新持有者。 |

## 使用路径

1. 先确定单进程 Memory 还是共享 Redis；配置校验与 init 成功后才使用。cache 不依赖 reldb。
2. 根据 KV/Hash/List/Set/ZSet 选择公开操作，明确 TTL、空值和回源失败的业务含义。
3. 读取 acquire 成功返回的唯一 owner，检查 success 与 data；续期/释放复用该 owner，丢锁后停止受保护工作。
4. 验证过期、连接失败、并发与锁过期后的旧任务；最后 close。

Cluster 的 mset/集合运算/多键 blocking pop 须同 slot；mget/del/exists 可跨 slot，但无全局原子性。scan 使用字符串游标遍历全部主节点，返回去除 keyPrefix 的逻辑 key；拓扑/条件变化后重新遍历。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [运行环境](reference.md#运行环境)
- [适用场景](reference.md#适用场景)
- [使用步骤](reference.md#使用步骤)
- [核心 API](reference.md#核心-api)
- [错误码 — `HaiCacheError`](reference.md#错误码--haicacheerror)
- [常见模式](reference.md#常见模式)
- [相关 Skills](reference.md#相关-skills)

单键 compareAndSwap(key, expected, value, {px?,keepTtl?}) 比较 JSON 序列化快照并原子替换，必须检查 success 与 boolean data。expected=undefined 表示不存在，value=undefined 表示删除，null 是值；px 正整数且与 keepTtl 互斥。比较失败不修改 TTL；跨节点版本状态优先使用 CAS 而不是租约后无条件 SET。

`kv.set` 返回 `HaiResult<boolean>`：`data=true` 表示实际写入，NX/XX 未命中返回 `success=true,data=false` 且不改变原值或 TTL。签发票据、会话或登记公钥前必须检查这两个字段。`ex/px/exat/pxat` 分别使用秒、毫秒、Unix 秒、Unix 毫秒，须为正安全整数且只能指定一种，与 `keepTtl=true` 互斥；`nx/xx` 也互斥。零、负数、小数、非有限值或溢出返回 `INVALID_SET_OPTIONS`（`cache:016`），不会写成永久键。过去的正绝对时间表示立即过期。省略期限会移除原 TTL；`keepTtl` 保留已有 TTL，新键仍永久保存。memory 和 Redis 都保存 JSON 快照。
