# 通用规范同步与本地联调

从 [hai-framework-sync](SKILL.md) 进入。仅同步 skills/AI 入口、修改通用模板或联调两个仓库时读取本文件；应用版本升级先执行入口中的版本发现与差异流程。

## 来源与边界

hai-framework 是通用能力和 skill 模板的源头；应用仓库消费这些能力，并保留项目私有规则。

| 任务 | 来源与修改范围 |
| --- | --- |
| 框架仓库开发规范与 skills | 框架根 `AGENTS.md`、`.agents/instructions/`、`.agents/skills/`；仓库私有路径不复制到应用模板 |
| 应用消费某发布版的 skills/AI 入口 | 对应精确版本 `@h-ai/cli` 的 `templates/skills/`；合并到当前授权应用，不要求修改框架仓库 |
| 修正可复用模块用法或工作流 | 在已授权框架仓库的 `packages/cli/templates/skills/` 修改源头，再同步受影响应用；核对相关仓库 skills 的公共契约 |
| 应用私有业务规则、产品决策与路径 | 留在应用 AGENTS 或本地 skill，不复制到通用模板 |
| 未发布模板/运行时联调 | 使用明确版本/提交的已授权本地框架来源，报告与 npm 发布版的差异 |

先确认目标应用的入口、实际目录、package.json/lockfile 和 npm/local 模式；有本地框架来源时再确认该仓库的入口与版本/提交。模板同步不证明新 API 已安装；示例必须符合目标依赖的公开 exports/types。通用内容不含敏感聊天记录、token 或私有地址。

## 同步完整技能与入口

通过 CLI 补齐缺失文件时，先按 [CLI 安装与命令](../hai-app-create/development.md#cli-安装与命令) 安装所需精确版本的 `@h-ai/cli` 并检查 `hai --version` / `hai --help`，再从应用根运行 `hai init`。临时使用 CLI 可隔离安装到临时目录；不为读取模板要求全局安装或改变系统级工具。

`hai init` 只补缺失内容，即使 `--force` 也不覆盖已有 AI 规则和 skills，不能用它代替旧版本规则合并。未发布模板修正从明确本地来源同步，不能假定已发布 CLI 已包含。

1. 比较来源与应用的同名完整目录，包括 SKILL、reference/guide、脚本和资产；升级时结合旧发布模板识别应用私有覆盖。模板业务默认值服从用户当前明确要求及其指定文档；移除或改写已被需求替代的旧模板强制规则，保留仍适用的项目私有决策，不将默认 admin/user、限额或流程重新强加给应用。无冲突部分合并，真实未决冲突列出位置、双方意图和可选方案；已有明确决策不重复询问。
2. 按目标架构选择需要的模块技能与桥接模板；不将 SvelteKit 指令套到 API/Vite 项目。保留自定义技能；上游重命名/删除的旧目录先检查调用与私有内容，不能整树覆盖或删除。
3. 项目规则合并到 AGENTS，`.github/copilot-instructions.md` 只引用共享入口；CLAUDE 如存在，通过 `@AGENTS.md` 复用共享规则，保留用户额外规则及现有 `.claude/`，不另复制技能正文。OpenCode 的 `instructions` 直接引用 `AGENTS.md`；配置有变化时合并已有字段，不整体替换。
4. 确保新增/修改/修复任务能到达 `hai-app-create` 与其 `development.md`，审查到达 `hai-app-review`，框架升级到达 `hai-framework-sync`。只引用目标实际交付的可选技能，不把目录缺失当成运行时能力缺失。

Codex 原生读取根 `AGENTS.md` 并发现 `.agents/skills/`，可用 `$hai-app-create` 等名称调用；不复制额外的 `.codex/skills`。同步后验证技能列表和按需参考文件可达；列表尚未刷新时重新打开项目或开始新会话。

## npm 与本地联调

默认 CI 验证 lockfile 中的已发布 npm 依赖，不依赖本机路径。先查应用实际 scripts；仅在存在时使用相应 `framework:*` 命令，否则遵循本项目 README，不凭空新增联调脚本：

```bash
pnpm framework:status
pnpm framework:use:local <package>
pnpm framework:watch <package>
```

PowerShell 使用 `pnpm.cmd`；占位包名替换后执行。本地来源先验证其版本/提交、构建产物与 exports，再开启联调；记录依赖映射/overrides 的恢复方式。联调结束按项目已有机制恢复 npm 并重装验证，不能把本地通过报告为发布版通过。

## 验证与交付

- 仅 skills/入口变更：检查 diff、完整目录、目标实际存在的入口及相邻资源、相对引用；排除框架仓库私有绝对路径/命令。
- 框架模板维护：运行已有 `check:skills` 和 CLI `test:scaffold-gates`，确认资源与路由在生成项目中可达。
- 同步应用依赖或运行时：按入口升级流程和 [分轮验证与集中修复](../hai-app-tests/validation.md) 收集适用门禁、集中修复并复测；CI/发布遵守 [hai-ci](../hai-ci/SKILL.md)，不以模板已同步代替运行验收。
- 双仓改动时说明来源版本/提交、改动顺序、应用私有覆盖和双方验证；涉及 PR 时按 [hai-pr-review](../hai-pr-review/SKILL.md) 关联同步说明。仅授权一个仓库时记录另一方待同步，不擅自修改。
