# hai-process-mix 到原生 HAI 插件迁移审计

审计日期：2026-07-28。

对照范围：

- 旧 Wasm：`hai-process-mix/plugins/mix/src/http_filter.rs`
- 旧转发 server：`hai-process-mix/server/src/{handler,headers,protocol}.rs`
- 原生实现：`hai-hub/backend/hai-hub-spacegate-plugins/src/plugin/hai/*.rs`

## 已对齐或本次修复

| 能力 | 旧实现 | 原生实现 | 结论 |
| --- | --- | --- | --- |
| API Key、IP/MAC、资产订阅校验 | Wasm 异步调用认证服务并缓存 | `hai-auth` 从 Redis 读取并校验身份、地址与资产授权 | 已迁移 |
| published 资产校验 | Wasm 在分发前拒绝非 published 资产 | `hai-asset` 拒绝非 published 资产 | 已迁移 |
| 静态内容和 URL 资产 | Wasm 直接构造响应 | `hai-dispatch` 返回文本或 URL JSON | 已迁移 |
| 资产密钥注入 | Header/query 注入 | `inject_asset_secrets` 注入 Header/query | 已迁移 |
| 上游 Host | server 从目标 URI 重写 Host | dispatch 从 runtime URI 重写 Host | 已迁移 |
| model/runtime/MCP path suffix | Wasm 将资产 ID 后的路径追加到 endpoint | 本次在 `append_runtime_path_suffix` 恢复相同规则，包括 `/mcp-services/<asset>/...` | 本次修复 |
| SSE 首包透传 | Wasm response body callback 逐 chunk `Continue`；server 原样 boxed body | 本次 `SseObserveBody` 逐 frame 透传 | 本次修复 |
| SSE usage | Wasm 在响应 body callback 处理流事件 | 本次仅在 EOF 解析 `[DONE]` 前最后一个 JSON event 的 usage | 本次按确认语义修复 |
| QPS 与并发 | Wasm 进程内令牌桶/并发计数，on_log/Drop 释放 | 本次改为 Redis 请求 lease、body EOF/Drop 释放、heartbeat/TTL 回收 | 本次修复 |

## 行为差异和遗漏

| 优先级 | 能力 | 旧实现 | 原生现状 | 风险与建议 |
| --- | --- | --- | --- | --- |
| P1 | 输出保护 | Wasm 对普通响应和 SSE 使用 `output_guard` 检查并可替换/截断内容 | `HaiObserveConfig.output_guard_enabled`、`allowed_output_targets` 仅是配置字段，代码注释已标记没有执行路径 | 安全策略在原生网关失效；应单独设计 response body guard，不能与本次 usage observer 混合实现 |
| P1 | 模型错误关键词 | Wasm 对非流式 model 响应检测 `model_error_keywords` | 原生 `model_error_keywords` 未被使用 | 调用审计仍可按 HTTP status 记录，但不会把模型 body 内错误识别为失败；应单独补回 |
| P1 | 旧 server 控制头剥离 | `build_upstream_request` 移除 Hop-by-Hop、`Hai-*` 和旧 Host | 原生 dispatch 直接进入 SpaceGate backend，只改写 Host；未统一删除 `hai-api-key`、`hai-*`、Connection 等控制头 | 可能泄露调用方 HAI API Key 或把网关控制头传给上游；需独立补充 request sanitize layer |
| P1 | 服务端审计上报 | Wasm 在结束时异步 dispatch report，并写 `ai_log` | 原生写 structured audit log 和 OpenTelemetry 指标；未看到同等 report HTTP 调用或 `ai_log` filter-state | 若下游依赖该上报/字段，需明确迁移目标并补充兼容层 |
| P2 | 资产/API Key 本地缓存 | Wasm RootContext 使用 5 分钟资产缓存和短期 API Key 缓存 | 原生每请求走 Redis client，没有等价的插件内缓存 | 功能正确但 Redis 压力和更新可见性不同；根据压测再决定是否增加缓存 |
| P2 | 上游 TLS 策略 | 旧 server 使用系统根证书 | 当前 SpaceGate 默认 client 使用关闭证书验证的配置 | 安全语义不同；应作为网关全局 TLS 策略单独整改，不能仅在 HAI 插件修复 |
| P2 | 流式总时长/空闲超时 | server timeout 只覆盖 request 到响应头，body 原样透传 | dispatch timeout 同样只包裹 inner response，SSE body 没有总时长或 idle timeout | 已获得响应头但永不发送 body 的流会持续占用真实并发 lease；这是保护语义，不是残留计数，若需要限制应新增显式 stream idle timeout |

## 发布要求

本次 quota 从旧字符串键 `hai:quota:concurrent:<asset_id>` 切换到 ZSET 键 `hai:quota:lease:<asset_id>`。发布时必须摘流或停止所有旧版网关实例，等待/终止旧在途请求，清理受影响资产的旧字符串键，再整体切换新镜像。旧、新版本不得长期混跑。

## 验收边界

本次完成后，SSE 首帧、最终 usage、lease 回收与 path suffix 有自动回归测试。输出保护、模型 body 错误识别、控制头剥离和审计 report 属于已确认的迁移遗漏，但不在本次实现范围，必须单独排期。
