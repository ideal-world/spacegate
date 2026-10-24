---
name: hai-core
description: "配置 hai 应用启动、YAML/环境变量、日志、i18n 与 HaiResult；排查配置覆盖和错误本地化。"
---

# hai-core

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/core 配置、日志、i18n、ID、Zod 错误本地化和 HaiResult |
| 适用场景 | 启动配置、日志脱敏、国际化或公共错误处理 |
| 输入 | CoreOptions、YAML、环境变量、locale、日志上下文 |
| 输出 | core.init 为同步 void；配置校验为 HaiResult；工具返回直接值 |
| 限制 | core 无 close；浏览器不加载 YAML。配置日志需脱敏，ID 不能当作密钥。 |

## 使用路径

- Node 启动用 core.init({ configDir }) 加载配置；勿传 logging 覆盖 _core.yml 中的日志设置，除非任务明确要求。
- 自动加载和监听精确跳过 `_deploy.yml` / `_deploy.yaml`，部署 CLI 按目标环境和凭据独立解析；业务 `deploy.yml`、手动 `config.load` 不受影响。单项 load 返回失败时记日志并继续，必需配置须 validate 并检查 success；目录扫描、日志设置等异常仍可能传播。
- 应用名称、版本、环境、默认语言和日志设置统一从 `_core.yml` 经 core.config 获取，日志使用 core.logger；UI 不另维护硬编码副本。SvelteKit layout / fullstack app.info 仅下发公开字段白名单；独立 Vite mobile-app 在 Node 构建时解析并注入公开默认值，修改这些编译默认值需要重新构建。静态 Web 的公开 API 地址等通过 `runtime-config.js` 覆盖时无需重建镜像，原生包在复制静态资源前准备配置入口。浏览器不加载 YAML，不接收完整模块配置、密钥或日志脱敏上下文。
- `HAI_<配置名>_<路径>` 可整体覆盖根/对象/数组；父节点优先，未覆盖时才遍历已有叶子。camelCase 不拆词，约定变量优先于显式插值。
- 环境文件采用六类：`.env.example` 完整列出字段用途、默认值、枚举和范围；`.env` 为公共基础；`.env.dev`、`.env.test`、`.env.prod` 为公共环境覆盖，这五类均提交 Git。`.env.local` 为本机私有配置，不提交 Git。优先级为进程环境 > `.env.local` > 环境文件 > `.env`；Vite 在 Node 配置阶段调用 `core.config.loadEnvFiles({ mode, envDir })` 并检查 success 后注入进程，Node 脚本按相同顺序传入 `--env-file`。`core.init` 不自行读取 dotenv。
- 配置 getter 与 ID/logger 方法不是 HaiResult；validate/load 等须检查 success。
- 日志可用 `{ error }` 记录原生 Error；两端先规范化诊断再字段/URL 脱敏，配置 redact 路径仍生效。原异常不变，错误子图循环/深度用固定标记；message/stack 自由文本中的秘密需调用方先清理或遮蔽整条路径。
- 初始化前创建的默认、模块与嵌套 child Logger 在后续输出中应用当前全局 logging；configure/setLevel 也作用于已有实例。create 的显式 level/format/context 优先，未指定项动态继承。浏览器 json/pretty 与附加 redact 路径实际生效，消息文本仍由调用方脱敏。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用步骤
- 核心 API
- 错误码
- 常见模式
- 相关 Skills
- 浏览器侧使用

Node 请求/SSR 必须用 `core.i18n.runWithLocale(locale, callback)`（kit 同名方法）包裹实际异步 resolve；serv 自动建立请求语言上下文。浏览器 setGlobalLocale/createLocaleStore 继续管理应用语言，SSR 的 UI setter/store 不写全局值。显式 MessageOptions.locale 优先；进程默认语言仅在启动时设置。
