# SpaceGate Hai 管理面迁移 — 会话交接文档

- 日期：2026-10-27
- 仓库：`/Users/yiye/projectSpace/huayun_project/spacegate-workspace/spacegate`
- 分支：`feat/hai-admin-migration`
- 新工程：`spacegate-admin/`
- 设计文档：`docs/superpowers/specs/2026-10-15-spacegate-hai-framework-migration-design.md`
- 进度看板：`docs/superpowers/progress/2026-10-24-hai-admin-migration-progress.md`
- 旧 UI 回归基线：`docs/superpowers/progress/2026-10-24-legacy-admin-operation-inventory.md`
- 本地联调手册：`docs/superpowers/migration/2026-10-24-hai-admin-local-test-with-all-in-one.md`
- 独立部署/切换：`docs/superpowers/migration/2026-10-24-spacegate-admin-independent-deploy-and-switch.md`

> 本文用于新会话接续。开始前先完整读本文，再读进度看板和设计文档。

---

## 1. 当前状态一句话

代码迁移阶段 P0–P6 已完成并通过全量本地验证；P4/P6 及文档**按要求尚未提交**，所有提交都只在本地，**没有 push**。
剩余工作主要是：UI 布局/登录插图补充、你本地验收后提交 P4/P6、原生验收、以及可选的独立仓库拆分。

---

## 2. 绝对不要做的事

1. **不要 push**。用户明确要求只本地提交，本地完全测试通过后再决定。
2. **不要 `git add -A` / `git add .`**。工作区含用户自己的未提交改动和未跟踪文档。
3. **不要修改 `spacegate-admin-fe`**。它是旧 Vue 管理端、真实验收基线和回滚入口。
4. **不要修改 `spacegate-admin-front`**。它只是外部参考仓库，不参与构建。
5. **不要提交** `docs/k8s/service-image-build-runbook.md`；这是用户原有改动。
6. 不要删除 `docs/superpowers/specs/`、`docs/superpowers/plans/` 等未跟踪文档。
7. 不要在未确认的情况下删除 `spacegate-admin/` 或执行仓库拆分后的清理。

---

## 3. 本地保护现场

用户原始未提交改动和未跟踪文档已备份到：

```text
/private/var/folders/kx/71r_87k50_78zjk1nsc50zz00000gn/T/opencode/spacegate-safe/
  tracked.diff
  status.txt
  stash-create.txt
  untracked-superpowers.tgz
```

该目录是临时目录，可能被系统清理；如果新会话发现现场异常，先检查这里，再决定是否恢复。

---

## 4. 环境

默认 Node 是 v24.8.0，**不满足** CLI 的 `^24.15.0`。必须使用 nvm 的 v24.22.1：

```bash
export PATH="$HOME/.nvm/versions/node/v24.22.1/bin:$PATH"
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
```

旧会话使用的 helper：

```bash
source /private/var/folders/kx/71r_87k50_78zjk1nsc50zz00000gn/T/opencode/hai-env.sh
```

如果 helper 不存在，用上面的 export 等价命令。

pnpm：

```bash
npx -y pnpm@12.15.0 <command>
```

工作区 pin 是 `12.11.2`，npx 会按 `packageManager` 使用该版本；不要用系统 pnpm 11.x 直接安装。

---

## 5. 已完成的本地提交

```text
ccdb5dc2 docs(admin): record P2/P3/P5 acceptance evidence
da98986f feat(admin): add native WeChat miniapp and constrained token channel
4db6aee2 feat(admin): add shared SpaceGate management pages and stores
ca08fdba feat(admin): add LDAP IAM, role guards and audit coverage
594ffcb5 feat(admin): add spacegate contract and upstream service adapter
c024520e chore(admin): scaffold Hai fullstack workspace on migration branch
```

提交只存在于本地 `feat/hai-admin-migration` 分支，没有远端跟踪，没有 push。

---

## 6. 未提交内容（P4/P6 + 后续修正）

截至 2026-10-27，`git status` 中的迁移相关未提交内容：

```text
M  docs/superpowers/progress/2026-10-24-hai-admin-migration-progress.md
M  spacegate-admin/.gitignore
M  spacegate-admin/README.md
M  spacegate-admin/e2e/fullstack.spec.ts
M  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/GatewayEditor.svelte
M  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/PluginEditor.svelte
M  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/RouteEditor.svelte
M  spacegate-admin/packages/spacegate-admin-shared/src/lib/stores/management-store.svelte.ts
M  spacegate-admin/playwright.config.ts

?? docs/superpowers/migration/
?? spacegate-admin/deploy/
?? spacegate-admin/e2e/mock-spacegate-upstream.mjs
?? spacegate-admin/e2e/spacegate.spec.ts
```

用户原有、**不属于本次迁移**的未提交改动：

```text
M  docs/k8s/service-image-build-runbook.md
```

另有未跟踪文档：

```text
?? docs/superpowers/plans/
?? docs/superpowers/specs/
```

### P4/P6 未提交的原因

用户要求先在本地完整测试，确认后再提交。不要擅自提交；等用户明确说“提交 P4/P6”再操作。

---

## 7. 架构与关键决策

### 7.1 拓扑

```text
Web / App / Desktop ──HTTPS──> /api/v1（SM2 transport）──> Rust admin-server
                                                  └──────> AI Gateway (/v1/admin)
微信小程序 ───────────────HTTPS──> /api/miniapp/v1（无 SM2，IAM + admin 守卫）
```

- 新控制面：`spacegate-admin-serv`（Node/Hai）。
- 数据平面/上游：Rust `spacegate` 网关、`spacegate-admin-server`、`ai-gateway-service`，不重写。
- 旧 Vue：`spacegate-admin-fe`，保留。
- 外部参考：`spacegate-admin-front`，不使用。

### 7.2 工程布局

```text
spacegate-admin/
  apps/spacegate-admin-web/        Svelte 5 + Vite
  apps/spacegate-admin-app/        Capacitor
  apps/spacegate-admin-desktop/    Tauri
  apps/spacegate-admin-miniapp/    微信原生 TS/WXML
  packages/spacegate-admin-contract/
  packages/spacegate-admin-shared/
  packages/spacegate-admin-serv/
  deploy/
  e2e/
```

### 7.3 关键语义

- 所有 SpaceGate procedures 都要求 IAM `admin` 角色。
- 写操作必须携带**调用方读取到的版本**；不能使用服务端全局最新版本。
- 编辑器打开时重新读取资源，建立 `gatewayEditVersion` / `routeEditVersion` / `pluginEditVersion` 快照。
- 409 不自动重试、不自动刷新草稿；显示版本冲突横幅，由用户点击“刷新后重试”。
- Rust 写成功不返回新版本；service 写后 GET `/config/names` 读取新版本。
- reload POST 也被 Rust 纳入版本控制，reload 后其他标签页可能 409，属于预期。
- 上游无认证只允许显式 `HAI_SPACEGATE_ALLOWINSECURE=true`；不能静默降级。
- 上游登录 200 但没有 JWT cookie 时拒绝继续，除非显式无认证。
- LDAP 默认关闭；`register.enabled=false` 关闭公开注册；`register.defaultEnabled=true` 让 LDAP 同步用户可登录；LDAP 用户只有 `user` 角色，admin 必须显式授予。
- 小程序通道关闭 SM2，仅靠 HTTPS + IAM + admin 守卫；必须内网/反代，`HAI_SERV_MINIAPP_PORT` 显式启用。

---

## 8. 验证证据（已完成）

| 检查 | 结果 |
|---|---|
| Root `pnpm typecheck` | PASS，0 errors/warnings |
| Root `pnpm lint` | PASS |
| Root `pnpm test` | PASS：contract 3、serv 41、shared 20、miniapp 10 |
| `pnpm build` / `pnpm compile` | PASS：service/contract/shared/web/app/desktop/miniapp |
| `pnpm test:e2e` | PASS：16 passed，1 skipped（OTP 通知夹具未配置） |
| `cargo test -p spacegate-admin-server` | PASS：13 passed |
| `docker compose config` | PASS（使用 deploy 示例 env） |

Playwright 覆盖：

- Web 登录 → 概览 → 网关 JSON 写 → 路由 JSON 写 → mock 上游状态校验。
- 409 冲突 → 横幅 → 显式 reload → 再保存成功 → 不自动重试。
- App 预览（5175）、Desktop 预览（5176）登录、概览、网关写。
- 非管理员只看到个人设置，管理 hash 路由回退到 settings。
- 旧 `fullstack.spec.ts` 已适配“公开注册关闭”和默认 `overview`。

运行命令：

```bash
cd spacegate/spacegate-admin
npx -y pnpm@12.15.0 typecheck
npx -y pnpm@12.15.0 lint
npx -y pnpm@12.15.0 test
npx -y pnpm@12.15.0 build
npx -y pnpm@12.15.0 test:e2e
```

Rust：

```bash
cd spacegate
cargo test -p spacegate-admin-server
```

---

## 9. 已知缺口

### 9.1 UI/视觉（用户已反馈）

- 页面功能基本可用，但**页面布局样例不足**。
- **登录页左侧插图缺失**。
- 目前是功能优先、布局最小可用，不是最终视觉稿。

这些是产品/设计层，不是框架缺陷。

### 9.2 原生验收未执行

- 微信开发者工具：CLI `islogin` 检测超时，IDE 未认证；未做 DevTools 编译/真机预览。
- Capacitor：`android/`、`ios/` 未生成；本机无 Android SDK，Xcode 未安装。
- Tauri：`bundle.targets=["nsis"]` 是 Windows 目标，本机无 Xcode；只做了 Web 资源构建。
- Docker 镜像实际构建未跑，只做了 compose 配置校验。

### 9.3 生产切换未执行

按设计，本分支不做生产切换、不迁移旧用户库、不删除旧工程。

---

## 10. 下一步任务（按优先级）

### 任务 A：补登录页左侧插图

不需要改框架代码，配置即可。

`spacegate-admin/packages/spacegate-admin-serv/config/_iam.yml`：

```yaml
ui:
  variant: split
  brandTitle: SpaceGate
  brandText: ...
  logoUrl: /logo.svg
  illustrationUrl: /auth-illustration.svg
```

把图片放到各端 `public/`：

```text
apps/spacegate-admin-web/public/auth-illustration.svg
apps/spacegate-admin-app/public/auth-illustration.svg
apps/spacegate-admin-desktop/public/auth-illustration.svg   # 如目录不存在则创建
```

或用绝对 HTTPS URL，避免每端放一份。改 `_iam.yml` 后重启 service。

`@h-ai/ui` 的 AuthPanel 读取 `iam.publicConfig.ui.illustrationUrl`，无需自定义登录组件。

### 任务 B：补页面布局模板

建议在 shared 新增：

```text
packages/spacegate-admin-shared/src/lib/components/layouts/
  ListPageLayout.svelte
  DetailPageLayout.svelte
  SplitEditorLayout.svelte
  ResponsiveGrid.svelte
  PageEmptyState.svelte
```

原则：

- 不改 contract/service，只改 shared 页面和组件。
- 用旧 Vue `spacegate-admin-fe` 和操作清单作为交互/视觉基线。
- 保持现有 `data-testid`，避免破坏 Playwright。
- 桌面/平板/手机断点要逐页检查，不要只做 1440px。
- 复杂编辑器的专用字段不要退化成单一 JSON 文本框。

如果需要具体设计稿，向用户索取：品牌色、字体、间距、目标屏幕、参考截图、登录插图素材。

### 任务 C：本地验收后提交 P4/P6

用户确认后，只提交迁移文件，明确排除：

```text
docs/k8s/service-image-build-runbook.md
docs/superpowers/plans/
docs/superpowers/specs/   # 除非用户要求
```

建议按主题拆两个本地 commit：

```bash
cd spacegate

git add \
  spacegate-admin/e2e/mock-spacegate-upstream.mjs \
  spacegate-admin/e2e/spacegate.spec.ts \
  spacegate-admin/e2e/fullstack.spec.ts \
  spacegate-admin/playwright.config.ts \
  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/GatewayEditor.svelte \
  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/PluginEditor.svelte \
  spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/RouteEditor.svelte \
  spacegate-admin/packages/spacegate-admin-shared/src/lib/stores/management-store.svelte.ts

git commit -m "test(admin): add browser acceptance and mock upstream"

git add \
  spacegate-admin/deploy \
  spacegate-admin/README.md \
  spacegate-admin/.gitignore \
  docs/superpowers/migration \
  docs/superpowers/progress/2026-10-24-hai-admin-migration-progress.md

git commit -m "docs(admin): add independent deploy and local test runbook"
```

不 push。

### 任务 D：可选独立仓库拆分

用户考虑把 `spacegate-admin` 拆成独立 Git 仓库，便于管理后台与网关分别发布。

前置条件：

1. P4/P6 已提交，工作区干净。
2. 用户明确同意拆分。
3. 保留历史，不要直接 `cp -r`。

推荐 `git subtree split`：

```bash
cd spacegate
git status --short          # 必须干净
git subtree split -P spacegate-admin -b spacegate-admin-standalone

mkdir -p ../spacegate-admin-repo
cd ../spacegate-admin-repo
git init
git pull ../spacegate spacegate-admin-standalone

# 本地 bare remote 验证（不是 GitHub push）
git init --bare ../spacegate-admin.git
git remote add origin ../spacegate-admin.git
git push -u origin main
```

干净克隆验证：

```bash
cd ..
git clone spacegate-admin.git spacegate-admin-clean
cd spacegate-admin-clean
npx -y pnpm@12.15.0 install
npx -y pnpm@12.15.0 typecheck
npx -y pnpm@12.15.0 test
npx -y pnpm@12.15.0 build
```

拆分后需要处理：

- 新仓库文档链接：`../../docs/superpowers/...` 改为新仓库 `docs/` 或网关仓库 URL。
- 网关仓库删除 `!spacegate-admin/scripts/` 忽略规则。
- `Dockerfile.all-in-one` 仍构建旧 `spacegate-admin-fe`；新 admin 独立发布，不再进 all-in-one。
- CI 分开：网关 Rust/镜像；新 admin contract/service/shared/前端/Playwright/镜像。
- 增加上游兼容矩阵：新 admin 版本 ↔ 已验证的 Rust admin-server commit/image digest。
- 不要在网关仓库用 submodule 嵌新 admin；用镜像 tag / 发布物耦合。

拆分风险：

- `subtree split` 只包含已提交内容；未提交的 P4/P6 会丢。
- 旧 `spacegate-admin-fe` 未提交的 `components/config` 改动不属于新仓库，不要误带。
- 独立仓库后，跨仓库改动需要先发 admin 兼容版本，再升级网关。

### 任务 E：原生验收

- 微信开发者工具：导入 `apps/spacegate-admin-miniapp`，填真实 AppID 到 `project.private.config.json`，关闭域名校验，设置 `HAI_SERV_MINIAPP_PORT=3001`，跑登录/概览/网关写/409/登出。
- Capacitor：安装 Android SDK / Xcode 后 `cap add android` / `cap add ios`，再 `cap sync`、原生构建。
- Tauri：在 Windows 主机用 NSIS 目标构建，或改 `tauri.conf.json` 的 bundle targets 后在 macOS 构建 `.app`。
- 所有原生结果单独报告，不能用浏览器预览替代。

---

## 11. 本地联调快速入口

完整手册：`docs/superpowers/migration/2026-10-24-hai-admin-local-test-with-all-in-one.md`

最小步骤：

```bash
# 1. 旧 all-in-one（作为上游/旧 UI）
docker run --rm --name spacegate-all-in-one \
  -p 9080:9080 -p 9991:9991 -p 9876:9876 -p 9000:9000 \
  -p 9992:9992 -p 18080:18080 \
  --add-host=host.docker.internal:host-gateway \
  -e REDIS_URL=redis://host.docker.internal:6379/0 \
  -v spacegate-config-test:/etc/spacegate \
  -v spacegate-native-plugins-test:/var/lib/spacegate/plugins \
  -v spacegate-wasm-test:/plugins \
  spacegate-all-in-one:local

# 2. 新管理面
cd spacegate/spacegate-admin
pnpm install
pnpm dev
```

`.env.local` 关键值：

```dotenv
HAI_SPACEGATE_ADMINBASEURL=http://127.0.0.1:9992
HAI_SPACEGATE_AIGATEWAYBASEURL=http://127.0.0.1:18080
HAI_SPACEGATE_ALLOWINSECURE=true
# 可选
# HAI_SERV_MINIAPP_PORT=3001
```

入口：

- 新 Web：<http://127.0.0.1:4173>
- 旧 Web：<http://127.0.0.1:9080>
- 新 service：<http://127.0.0.1:3000>
- 小程序通道：<http://127.0.0.1:3001/api/miniapp/v1>

all-in-one 的 `spacegate-admin-server` 没有 `--key/--sk`，所以新 service 必须 `ALLOWINSECURE=true`；如果以后给 all-in-one 开启认证，改为 ak/sk 并设 `ALLOWINSECURE=false`。

---

## 12. 关键文件地图

### Contract / service

```text
spacegate-admin/packages/spacegate-admin-contract/src/spacegate-schemas.ts
spacegate-admin/packages/spacegate-admin-contract/src/spacegate-contract.ts
spacegate-admin/packages/spacegate-admin-serv/src/server/upstream/spacegate-upstream-client.ts
spacegate-admin/packages/spacegate-admin-serv/src/server/procedures/spacegate-procedures.ts
spacegate-admin/packages/spacegate-admin-serv/src/server/procedures/admin-procedures.ts
spacegate-admin/packages/spacegate-admin-serv/config/_spacegate.yml
spacegate-admin/packages/spacegate-admin-serv/config/_iam.yml
spacegate-admin/packages/spacegate-admin-serv/config/_serv.yml
```

### Shared / UI

```text
spacegate-admin/packages/spacegate-admin-shared/src/lib/stores/management-store.svelte.ts
spacegate-admin/packages/spacegate-admin-shared/src/lib/pages/{overview,gateways,routes,plugins,ai-queue,instances}.svelte
spacegate-admin/packages/spacegate-admin-shared/src/lib/components/spacegate/*.svelte
spacegate-admin/packages/spacegate-admin-shared/src/lib/components/AdminWorkspace.svelte
spacegate-admin/packages/spacegate-admin-shared/src/lib/components/AppShell.svelte
spacegate-admin/packages/spacegate-admin-shared/messages/*.json
```

### Miniapp

```text
spacegate-admin/apps/spacegate-admin-miniapp/miniprogram/**
spacegate-admin/apps/spacegate-admin-miniapp/miniprogram/lib/api.ts
spacegate-admin/apps/spacegate-admin-miniapp/tests/**
```

### Tests / deploy

```text
spacegate-admin/e2e/mock-spacegate-upstream.mjs
spacegate-admin/e2e/spacegate.spec.ts
spacegate-admin/deploy/compose.yaml
spacegate-admin/deploy/spacegate-admin.env.example
spacegate-admin/deploy/generate-transport-keyring.mjs
```

---

## 13. 常见坑

1. **Node 版本**：默认 24.8 不满足 engine，必须 nvm 24.22.1。
2. **pnpm 版本**：系统 pnpm 11 不满足 `>=12.11.2`，用 npx。
3. **不要 `git add -A`**：会带入用户 k8s 文档和未跟踪 plans/specs。
4. **all-in-one 端口**：新 service 需要 `-p 9992:9992 -p 18080:18080`，否则连不上上游。
5. **上游认证**：all-in-one 无认证；新 service 必须显式 `ALLOWINSECURE=true`。
6. **版本冲突**：409 是预期，不要加自动重试；必须用户显式刷新。
7. **编辑器版本快照**：不要改回全局 `store.version` 写操作，会绕过并发保护。
8. **旧 front**：不要改；它不参与构建。
9. **旧 fe**：不要改；它是回滚入口，当前 all-in-one 仍构建它。
10. **小程序通道**：没有 SM2，只能 HTTPS/内网；不要暴露公网。

---

## 14. 新会话开始检查清单

```bash
cd /Users/yiye/projectSpace/huayun_project/spacegate-workspace/spacegate

git status --short
git log --oneline -7
git branch --show-current

# 读文档
sed -n '1,220p' docs/superpowers/progress/2026-10-24-hai-admin-migration-progress.md
sed -n '1,220p' docs/superpowers/migration/2026-10-24-hai-admin-local-test-with-all-in-one.md
```

然后：

1. 确认用户是否已本地验收；
2. 确认是否提交 P4/P6；
3. 确认是否开始仓库拆分；
4. 确认 UI 布局/插图是否有设计稿或参考页面；
5. 任何删除、提交、push 前先征得用户同意。

---

## 15. 验收标准（下一阶段）

- 登录页左侧插图可配置、Web/App/Desktop 均能显示或使用绝对 URL。
- 页面布局在 1440/1024/768/390 宽度下无横向溢出，列表/编辑器可用。
- 现有 `data-testid` 和 Playwright 用例保持通过。
- P4/P6 提交内容不含用户 k8s 文档、plans/specs。
- 若拆分仓库：干净克隆可 `pnpm install && typecheck && test && build`，网关与新 admin 可分别打 tag/镜像。
- 原生验收结果单独报告，不把浏览器预览当原生通过。
