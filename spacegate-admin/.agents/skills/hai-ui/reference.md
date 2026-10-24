# hai-ui — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/ui` 是基于 Svelte 5 Runes 的多端 UI 组件库，采用 DaisyUI v5 + Tailwind CSS v4 + Bits UI v2，内置 15 个精选 DaisyUI 主题、内置中英日文 i18n、自动导入。内置 Shiki 代码高亮、Mermaid 图表渲染、Design Token 系统和 7 个移动端组件。

## 运行环境

> **面向 Svelte / SvelteKit 界面层。** 组件可用于 SvelteKit 的 SSR + CSR 页面，也可用于纯客户端 SPA / 原生壳 App；不要在纯 Node 服务模块里直接引用 UI 组件。

## 适用场景

- 构建管理后台页面（表单、表格、弹窗、导航等）
- 移动端/App 界面开发（SafeArea、BottomNav、PullRefresh 等）
- 使用 Bits UI headless 交互组件（Combobox、DatePicker、Calendar）
- 使用 IAM 场景组件（登录/注册/密码/权限守卫/用户资料表单）
- 使用 Storage 场景组件（文件上传/图片上传/头像上传/文件列表）
- 使用 CRUD 场景组件（列表过滤、详情/编辑面板、删除确认）
- 渲染 AI 输出（Markdown / Mermaid 文档、代码产物预览）
- 配置主题切换与 i18n 多语言
- 多端平台检测与适配

## 使用边界

- 加密输入变更会清空密文，乱序/清空/卸载后的旧结果不会回填；失败显示错误。提交前确认当前密文非空，不缓存上次密文。

- `EncryptedInput` 的 `onencrypt` 必须接入真实加密服务；`algorithm` 仅标注算法。缺少回调时显示不可用，不生成密文，不允许用编码代替加密。

- **优先复用 `@h-ai/ui` 现有组件**，不要在 app 中重复实现同类按钮、表格、表单、弹层或场景页面。
- **页面级文案与业务逻辑放在应用层**；`@h-ai/ui` 负责展示与交互，不承接业务服务、数据库访问或页面级 i18n 管理。
- **场景组件自带内置中英日文文案**（如 IAM / CRUD / AI / Storage 场景），除非组件显式提供覆盖 prop，否则不要把页面翻译样板再传进去。
- **自动导入不是全能魔法**：`toast`、类型导入和 `Range` 仍需显式 `import`；其它公开 Svelte 组件可交给 `autoImportHaiUi()`。
- **移动端组件不是样式糖**：使用 `SafeArea` / `BottomNav` / `PullRefresh` / `ActionSheet` 时，务必同步引入 `design-tokens.css` 与 `mobile.css`。

## AI 使用顺序（先判断，再落地）

### 1. 先确认项目集成是否完整

至少检查以下四项：

- `svelte.config.*`：`autoImportHaiUi()` 在 `vitePreprocess()` 前。
- `vite.config.*`：`ssr.noExternal` 包含 `@h-ai/ui` 和 `@h-ai/kit`，`optimizeDeps.exclude` 包含 `bits-ui`。
- `app.css`：已导入 `global.css` / `theme.css`；移动端还要导入 `design-tokens.css` / `mobile.css`。
- Tailwind：已配置 `@source` 扫描 `@h-ai/ui/dist/**/*.{svelte,js,ts}`。

如果这四项不完整，优先先补集成，再写页面；否则组件很容易出现“能编译但没样式 / SSR 报错 / 自动导入失效”。

### 2. 再选组件层级

按复杂度从低到高选：

- **primitives**：单个交互单元，例如 `Button`、`Input`、`Select`、`Badge`。
- **compounds**：通用业务骨架，例如 `Form`、`Modal`、`Drawer`、`DataTable`、`Pagination`。
- **scenes**：完整业务流，例如 `LoginForm`、`FileUpload`、`CrudPage`、`AiDocumentEditor`。
经验法则：

- 只是做一个输入控件或按钮行 → 先看 primitives。
- 需要通用弹层/表格/分页/表单布局 → 先看 compounds。
- 已经是“登录页 / CRUD 页 / 文件上传页 / AI 文档页”这类完整场景 → 先看 scenes，**不要手搓一遍**。

### 3. 常见任务 → 首选组件

| 任务 | 首选组件 | 使用要点 |
| --- | --- | --- |
| 后台列表页 | `PageHeader` + `Card` + `DataTable` + `Pagination` | 分页、筛选、批量操作优先复用 compounds |
| 表单页/弹层表单 | `Form` + `FormField` + `Input/Select/...` | 表单字段布局统一交给 `FormField` |
| 简单确认/详情弹层 | `Modal` / `Drawer` / `Confirm` | 根据桌面/移动端交互选择弹窗或抽屉 |
| 完整 CRUD 页面 | `CrudPage` | 优先通过 `form` / `pagination` / `density` 配置，不要拆开重写；列头默认可排序、筛选栏自带「重置」 |
| 错误页（401/403/404/500/503） | `ErrorPage` | SvelteKit `+error.svelte` 中按 `page.status` 使用，`onhome`/`onback` 接管跳转 |
| 设置页 | `SettingsLayout` | 分区导航 + 内容区；`sections` + `active` + `onselect`，内容放入 children |
| 登录/注册页布局 | `AuthShell` + `LoginForm` / `RegisterForm` | `variant='card'` 或 `'split'`；split 可通过 `illustration`/`description`/`highlights` 放大图与重点说明 |
| 登录/注册/资料页 | `LoginForm` / `RegisterForm` / `UserProfile` | 页面只负责路由和提交逻辑 |
| 文件/图片上传 | `FileUpload` / `ImageUpload` / `AvatarUpload` | 上传 URL、限制和业务状态由应用层提供 |
| 移动端页面骨架 | `SafeArea` + `AppBar` + `BottomNav` | 原生壳页面优先这一套 |
| AI 文档/Markdown 展示 | `MarkdownRenderer` / `AiDocumentEditor` | Mermaid、代码高亮、复制/下载已内置 |

### 4. 最后再决定是否自定义样式

- 优先通过组件已有 props（`variant` / `size` / `outline` / `class` / snippet slot）调整。
- 需要统一视觉规范时，优先改 `theme.css` / `design-tokens.css` 对应 token，而不是在页面里散落 magic class。
- 只有当 `@h-ai/ui` 现有抽象确实不覆盖时，才在应用层补专属组件。

## 项目配置（从 npm 安装）

> 以下示例面向通过 `npm install @h-ai/ui` 引用发布包的项目。monorepo 内部使用 `workspace:*` 时路径略有不同。

### 1. 安装依赖

```bash
# 核心包
npm install @h-ai/ui @h-ai/core

# 必须的对等依赖与构建工具
npm install -D svelte @sveltejs/kit @sveltejs/vite-plugin-svelte
npm install -D tailwindcss @tailwindcss/vite daisyui

# 图标（可选但推荐）
npm install -D @iconify/tailwind4 @iconify-json/tabler
```

Node adapter 使用 Crypto、Kit 或 IAM 时，还需将 `sm-crypto@0.5.7` 作为直接生产依赖安装，保留 Node 原生模块解析。

### 2. svelte.config.js

```js
import { autoImportHaiUi } from '@h-ai/ui/auto-import'
import adapter from '@sveltejs/adapter-node'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

/** @type {import('@sveltejs/kit').Config} */
const config = {
  // autoImportHaiUi 必须在 vitePreprocess 之前
  preprocess: [autoImportHaiUi(), vitePreprocess()],
  compilerOptions: {
    runes: true, // 强制 Svelte 5 Runes 模式
  },
  kit: {
    adapter: adapter(),
    alias: {
      $components: './src/lib/components',
      $stores: './src/lib/stores',
    },
  },
}

export default config
```

### 3. vite.config.ts

```ts
import { sveltekit } from '@sveltejs/kit/vite'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [
    sveltekit(),
    tailwindcss(),
  ],
  optimizeDeps: {
    // bits-ui 使用 Svelte 虚拟模块，需排除预构建
    exclude: ['bits-ui'],
  },
  ssr: {
    // @h-ai 包为 ESM + Svelte 组件，需 Vite 在 SSR 时处理
    noExternal: ['@h-ai/ui', '@h-ai/kit'],
    external: ['@h-ai/core', 'sm-crypto'],
  },
})
```

### 4. src/app.css

```css
/* ─── Tailwind CSS v4 ─── */
@import 'tailwindcss';

/* ─── @h-ai/ui 共享样式 ─── */
@import '@h-ai/ui/styles/global.css';   /* 基础重置、滚动条、焦点样式 */
@import '@h-ai/ui/styles/theme.css';    /* Tailwind v4 @theme Token（品牌色/阴影/动效） */

/* ─── 移动端项目追加（可选） ─── */
@import '@h-ai/ui/styles/design-tokens.css'; /* CSS 自定义属性：间距/圆角/z-index */
@import '@h-ai/ui/styles/mobile.css';        /* 安全区域/触摸优化/键盘适配 */

/* ─── 扫描 @h-ai/ui 组件中使用的 Tailwind 类名 ─── */
@source "../node_modules/@h-ai/ui/dist/**/*.{svelte,js,ts}";
@source "../../../node_modules/@h-ai/ui/dist/**/*.{svelte,js,ts}";

/* ─── DaisyUI 主题 ─── */
@plugin "daisyui" {
  themes:
    light --default,
    dark --prefersdark,
    cupcake,
    emerald,
    corporate,
    nord,
    dracula,
    night,
    dim,
    business,
    sunset;
}

/* ─── 图标（可选） ─── */
@plugin "@iconify/tailwind4" {
  prefixes: tabler;
}
```

> **关键**：`@source` 行让 TailwindCSS 扫描 `@h-ai/ui` 组件中使用的 class 名，否则组件样式会丢失。路径指向 npm 安装目录下的 `dist/`。

### 5. app.html 防闪烁脚本

```html
<!doctype html>
<html lang="%lang%">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <script>
    // 防止主题切换闪烁：在 DOM 渲染前应用已保存的主题
    (function(){var t='light';try{var s=localStorage.getItem('theme');if(s)t=s}catch{}document.documentElement.setAttribute('data-theme',t)})()
  </script>
  %sveltekit.head%
</head>
<body data-sveltekit-preload-data="hover">
  <div style="display: contents">%sveltekit.body%</div>
</body>
</html>
```

### 6. 平台检测

```typescript
import { detectPlatform, isMobile, isNativeApp, usePlatform } from '@h-ai/ui'

// 一次性检测
const platform = detectPlatform() // 'ios' | 'android' | 'web'
const mobile = isMobile() // boolean
const native = isNativeApp() // boolean（Capacitor 环境）

// Svelte 5 响应式（组件中使用）
const p = usePlatform()
// p.platform / p.isMobile / p.isNative
```

## 三层组件架构

### 原子组件（Primitives，20 个）

| 组件         | Props 要点                                                    | 说明     |
| ------------ | ------------------------------------------------------------- | -------- |
| `Button`     | `variant`, `size`, `loading`, `disabled`, `outline`, `circle` | 按钮     |
| `IconButton` | `icon: trusted SVG string \| Snippet`, `tooltip`, `variant`, `size` | 图标按钮 |
| `Input`      | `value`, `type`, `size`, `error`, `placeholder`               | 输入框   |
| `Textarea`   | `value`, `rows`, `autoResize`, `error`                        | 文本域   |
| `Select`     | `value`, `options: SelectOption[]`, `placeholder`             | 下拉选择 |
| `Checkbox`   | `checked`, `label`, `indeterminate`                           | 复选框   |
| `Switch`     | `checked`, `label`, `size`                                    | 开关     |
| `Radio`      | `value`, `options`, `direction`                               | 单选组   |
| `Range`      | `value`, `min`, `max`, `step`, `variant`, `size`              | 滑块     |
| `Rating`     | `value`, `max`                                                | 评分     |
| `Badge`      | `variant`, `size`, `outline`                                  | 徽标     |
| `Avatar`     | `src`, `name`, `size`, `shape`                                | 头像     |
| `Tag`        | `text`, `variant`, `closable`                                 | 标签     |
| `Spinner`    | `size`, `variant`                                             | 加载动画 |
| `Progress`   | `value`, `max`, `striped`, `animated`                         | 进度条   |

### 组合组件（Compounds，36 个）

由原子组件 + Bits UI headless 交互组合。

#### 桌面端组合组件

| 组件         | Props 要点                                                         | 说明          |
| ------------ | ------------------------------------------------------------------ | ------------- |
| `Form`       | `columns`, `labelPosition`, `labelWidth`, `loading`, `disabled`, `onsubmit`                                  | 表单容器      |
| `FormField`  | `label`, `for`, `colSpan`, `labelPosition`, `error`, `hint`, `required`                       | 表单字段      |
| `Modal`      | `open`, `title`, `size`, `closeOnBackdrop`                         | 模态框，内置对话框语义、焦点约束和关闭后焦点恢复 |
| `Drawer`     | `open`, `position`, `size`, `width`, `resizable`, `widthStorageKey` | 可选拖动并记忆宽度的抽屉 |
| `DataTable`  | `data`, `columns`, `keyField`, `sortKey`, `sortDir`, `onsort`, snippet slots | 数据表格（列定义 `sortable` 可排序） |
| `Combobox`   | `options`, `value`, `multiple`, `placeholder`, `error`, `onchange` | 可搜索选择    |
| `Calendar`   | `value`, `minValue`, `maxValue`                                    | 独立日历      |
| `DatePicker` | `value`, `minValue`, `maxValue`, `error`                           | 日期输入+弹出 |
| `Tabs`       | `items: TabItem[]`, `active`, `type`                               | 标签页        |
| `Pagination` | `page`, `total`, `pageSize`, `showTotal`, `showJumper`, `showSizeChanger`, `showPageInfo`, `showFirstLast`, `onchange` | 分页（统一为 shadcn table 风格） |
| `Dropdown`   | `items: DropdownItem[]`, `trigger`                                 | 下拉菜单      |
| `Accordion`  | `items: AccordionItem[]`                                           | 折叠面板      |
| `Skeleton`   | `variant`, `count`, `animation`                                    | 骨架屏        |
| `Empty`      | `title`, `description`, `icon`                                     | 空状态        |

#### 移动端组合组件（7 个）

| 组件             | Props 要点                                                                    | 说明             |
| ---------------- | ----------------------------------------------------------------------------- | ---------------- |
| `SafeArea`       | `top`, `bottom`, `left`, `right`                                              | 安全区域容器     |
| `AppBar`         | `title`, `backHref`, `onback`, `fixed`, `transparent`, snippet `left`/`right` | 顶部导航栏       |
| `BottomNav`      | `items: BottomNavItem[]`, `active`                                            | 底部导航栏       |
| `ActionSheet`    | `open`, `title`, `items: ActionSheetItem[]`, `cancelText`, `onselect`         | 底部弹出操作面板 |
| `PullRefresh`    | `refreshing`, `onrefresh`, `threshold`, `pullText`, `releaseText`             | 下拉刷新         |
| `InfiniteScroll` | `loading`, `finished`, `threshold`, `onload`, `loadingText`, `finishedText`   | 无限滚动加载     |
| `SwipeCell`      | `leftActions`, `rightActions: SwipeCellAction[]`, `threshold`                 | 滑动操作单元格   |

**移动端组件用法示例**：

```svelte
<script lang="ts">
  import type { BottomNavItem } from '@h-ai/ui'

  const navItems: BottomNavItem[] = [
    { key: 'home', label: '首页', icon: 'tabler:home', href: '/' },
    { key: 'discover', label: '发现', icon: 'tabler:compass', href: '/discover' },
    { key: 'profile', label: '我的', icon: 'tabler:user', href: '/profile' },
  ]
</script>

<SafeArea top bottom>
  <AppBar title="首页" />
  <PullRefresh bind:refreshing onrefresh={loadData}>
    <main class="p-4">
      <!-- 页面内容 -->
    </main>
  </PullRefresh>
  <BottomNav items={navItems} active="home" />
</SafeArea>
```

**ActionSheet 用法**：

```svelte
<script lang="ts">
  import type { ActionSheetItem } from '@h-ai/ui'

  let showActions = $state(false)
  const actions: ActionSheetItem[] = [
    { key: 'camera', label: '拍照' },
    { key: 'album', label: '从相册选择' },
    { key: 'delete', label: '删除', destructive: true },
  ]
</script>

<ActionSheet bind:open={showActions} title="选择操作" items={actions} onselect={handleAction} />
```

**SwipeCell 用法**：

```svelte
<script lang="ts">
  import type { SwipeCellAction } from '@h-ai/ui'

  const rightActions: SwipeCellAction[] = [
    { key: 'edit', label: '编辑', color: '#3b82f6' },
    { key: 'delete', label: '删除', color: '#ef4444' },
  ]
</script>

<SwipeCell {rightActions} onaction={handleSwipeAction}>
  <div class="p-4">列表项内容</div>
</SwipeCell>
```

### 场景组件（Scenes）

内置中英日文翻译的业务场景组件。

#### 引导与帮助场景组件

| 组件 | Props 要点 | 说明 |
| --- | --- | --- |
| `GuideHost` | `guide`, `locale`, `scope`, `autoStart` | 首次邀请、操作导览和帮助弹窗 |
| `HelpCenter` | `articles`, `tours`, `locale`, `selectedId`, `onstart` | 可独立使用的搜索帮助中心 |

控制器与导航、偏好和业务 i18n 契约见 [guide.md](guide.md)。

#### IAM 场景组件

| 组件           | Props 要点                                      | 说明     |
| -------------- | ----------------------------------------------- | -------- |
| `AuthShell`    | `variant`('card'/'split'), `title`, `brandTitle`, `brandText`, `illustration`, `description`, `highlights` | 认证页布局（包裹登录/注册表单） |
| `LoginForm`    | `showForgotPassword`, `showRegisterLink`, `errors`  | 登录表单 |
| `RegisterForm` | `fields`, `minPasswordLength`, `errors`         | 注册表单 |
| `UserProfile`  | `user`, `editable`, `fields`, `avatarAccept`, `avatarHint` | 用户资料，选择器和提示对齐业务校验；最终验证和上传仍由回调负责 |

#### 错误页 / 设置场景组件

| 组件             | Props 要点                                  | 说明     |
| ---------------- | ------------------------------------------- | -------- |
| `ErrorPage`      | `status`, `code`, `title`, `description`, `homeUrl`, `onhome`, `onback` | 通用错误页（内置 401/403/404/500/503） |
| `SettingsLayout` | `title`, `description`, `sections`, `active`, `onselect` | 设置页分区导航布局 |

#### Storage 场景组件

| 组件          | Props 要点                                     | 说明     |
| ------------- | ---------------------------------------------- | -------- |
| `FileUpload`  | `accept`, `maxSize`, `uploadHandler`, `autoUpload` | 文件上传 |
| `ImageUpload` | `value`, `uploadHandler`, `aspectRatio`            | 图片上传 |
| `AvatarUpload` | `value`, `uploadHandler`, `size`, `fallback`      | 头像上传 |
| `FileList`    | `files: FileItem[]`, `layout`, `showPreview`   | 文件列表 |

上传协议属于应用 service：组件通过 `uploadHandler(file, { signal, onProgress })` 注入实现，并接收 `{ url?, response? }`。不要把 presign、认证头或 PUT/POST 细节塞进 UI 组件；图片与头像 handler 必须返回安全的 `url`。未提供 handler 时组件只负责文件选择或本地预览。

#### AI 场景组件

| 组件                 | 说明                              |
| -------------------- | --------------------------------- |
| `MarkdownRenderer`   | Markdown 渲染（内置 Shiki 高亮，支持 `fontSize` / `allowHtmlTags`） |
| `AiDocumentDownloadMenu` | AI 文档下载菜单              |
| `AiDocumentEditor`   | AI 文档编辑器（支持 Mermaid、受控代码预览，以及 `fontSize` / `allowHtmlTags`） |
| `AiTableEditor`      | AI 表格编辑器                    |

> AI 场景组件使用 Shiki（纯 ESM）进行代码高亮，支持 27 种语言，通过 CSS 变量 `--hai-hl-*` 自定义颜色。无需额外安装 Shiki，已内置。`AiDocumentEditor` 默认只允许 Markdown 内置预览；HTML / JS / CSS 等高风险预览需要显式启用 `allowUnsafeCodePreview`，或通过 `oncoderun` 返回受控的预览结果。
>
> Mermaid 开箱即用：
> - `sourceKind='document'` 时，文档中的 ```` ```mermaid ```` 代码块会在阅读态自动渲染为图表；
> - `sourceKind='code'` + `showCodePreviewToggle` 时，可在「代码 / 预览」之间切换查看 Mermaid 图表；
> - Mermaid 以 `securityLevel: 'strict'` 渲染为消毒后的 SVG，无需开启 `allowUnsafeCodePreview`。
>
> 展示层还支持：
> - `fontSize`：支持 `number`（按 px）或 CSS 长度字符串（如 `1.125rem`）
> - `allowHtmlTags`：默认关闭；开启后按安全白名单解析 `<b>` / `<i>` / `<u>` / `<mark>` 等标签，危险标签与属性仍会被消毒

```svelte
<MarkdownRenderer content={markdown} fontSize='1.125rem' allowHtmlTags />

<AiDocumentEditor
  title="方案文档"
  content={documentMarkdown}
  fontSize={18}
  allowHtmlTags
  showToolbar
  showOutline={false}
/>
```

#### Charts 图表

`Chart type='line'` 默认使用 LayerChart 原生折线图；监控看板类场景需要“分段线 + 圆点 + 最近点 hover + 竖向参考线”时，使用 `lineVariant='segmented-point'`：

```svelte
<Chart
  type='line'
  data={trendData}
  x='label'
  y={['rateLimit', 'circuitBreak']}
  series={trendSeries}
  lineVariant='segmented-point'
  tooltipMode='nearest'
  showCrosshair
  legend
  grid
/>
```

#### CRUD 场景组件

| 组件                | 说明 |
| ------------------- | ---- |
| `CrudPage`          | CRUD 主页面 |
| `CrudFilterBar`     | 过滤工具栏 |
| `CrudDetailPanel`   | 详情面板（抽屉/弹窗） |
| `CrudEditPanel`     | 编辑面板（抽屉/弹窗） |
| `CrudDeleteConfirm` | 删除确认框 |

`CrudPage` 是首选入口；只有在你明确要自定义组合方式时，才直接使用 `CrudFilterBar` / `CrudDetailPanel` / `CrudEditPanel`。

### CrudPage 配置重点

| 配置项 | 说明 |
| --- | --- |
| `form.variant` | `'drawer'`（抽屉，默认）或 `'modal'`（弹出窗口） |
| `form.drawerSize` / `form.drawerWidth` / `form.drawerResizable` | 抽屉尺寸预设 / 自定义 CSS 宽度 / 是否允许拖动并按资源记忆宽度（默认允许） |
| `form.modalSize` / `form.modalWidth` / `form.modalHeight` | 弹窗尺寸预设 / 自定义宽高 |
| `pagination.showSizeChanger` | 每页条数选择器（默认开启） |
| `pagination.pageSizeOptions` | 每页条数候选项（默认 `[10, 20, 50, 100]`） |
| `pagination.showJumper` / `pagination.showTotal` | 跳页输入 / 总数（默认开启） |
| `pagination.showPageInfo` / `pagination.showFirstLast` | 页码文案 / 首页末页按钮（默认开启；紧凑视图可关闭） |
| `density` | 列表密度：`'normal'`（默认）或 `'compact'`；仅影响列表行、行内操作、分页与创建/编辑/详情表单 |
| `toolbarStyle` | `'toolbar'`（图标弹层，默认）或 `'filter-bar'`（平铺筛选栏） |
| `showHeader` / `toolbarLeading` | 关闭默认页头并在工具栏左侧放置标题或说明 |
| `toolbarActions` | 工具栏业务操作 snippet |
| `card` | 卡片列表项渲染 snippet；仍复用分页和独立滚动容器 |
| `detailColumns` | 点击后打开详情抽屉的列 key，例如 `['title']` |
| `sortableColumns` | 可执行服务端排序的列；为空时使用全部列表列 |
| `loading` / `loaded` / `error` / `onRetry` / `keepPreviousData` | 受控加载态：仅 `loaded && total===0` 显示空状态；首次显示查询中；失败显示错误+重试；`keepPreviousData`（默认 true）失败保留旧数据 |

注意：`CrudPage` 会占满父级可用高度；表格无论是否有数据都会撑满剩余空间，仅数据区滚动，表头和分页栏固定。

### CrudPage 推荐用法

```svelte
<CrudPage
  crud={roleCrud}
  {data}
  permissions={{ create: true, update: true, delete: true }}
  form={{ variant: 'modal', modalSize: 'lg' }}
  pagination={{ showSizeChanger: true, showJumper: true, pageSizeOptions: [10, 20, 50] }}
  density='compact'
  detailColumns={['name']}
  sortableColumns={[{ key: 'createdAt', label: '创建时间' }]}
  {nav}
/>
```

如果想维持侧边编辑体验，可改成：

```svelte
form={{ variant: 'drawer', drawerWidth: '40rem' }}
```

抽屉默认可拖动左侧边缘调整宽度，并按 `crud.resource.name` 记忆；传 `drawerResizable: false` 可关闭。

### 纯客户端控制器与惰性页签

- 无 SvelteKit SSR 时用 `defineCrud` + `createCrudController` 托管 `data` / `nav`：latest-wins（seq + AbortSignal）、失败保留旧数据、并发去重、查询串回填地址栏。`<CrudPage data={c.data} nav={c.nav} loading={c.loading} loaded={c.loaded} error={c.error} onRetry={c.reload} />`。
- `Tabs` 传 `lazy` 后未激活页签不渲染（`children` 提供 `isActivated(key)`），`onactivate(key)` 首次激活触发一次；配合 `createCrudController({ autoLoad: false })` 在页签首次可见时才 `load()`。
- Toast 已内置 `notifySuccess` / `notifyError(err, fallback?)` / `notifyInfo` / `notifyWarning`，业务应用无需自建 toast 工具文件。

## Design Token 系统

### theme.css — Tailwind v4 `@theme` Token

通过 `@import '@h-ai/ui/styles/theme.css'` 导入，提供全局设计 Token：

```css
@theme {
  --color-brand: oklch(0.6 0.2 275);
  --shadow-xs / --shadow-soft / --shadow-lifted / --shadow-float / --shadow-overlay
  --ease-out-expo / --ease-in-out
  --font-feature-tabular: 'tnum';
}
```

应用可在导入后追加自己的 `@theme` 块覆盖或扩展。

### design-tokens.css — CSS 自定义属性

```css
--hai-spacing-xs: 4px;   --hai-spacing-sm: 8px;
--hai-spacing-md: 16px;  --hai-spacing-lg: 24px;
--hai-radius-sm: 4px;    --hai-radius-md: 8px;
--hai-touch-target-min: 44px;
--hai-safe-area-top: env(safe-area-inset-top);
--hai-z-dropdown: 1000;  --hai-z-modal: 2000;  --hai-z-toast: 3000;
--hai-transition-fast: 150ms ease;
--hai-transition-normal: 250ms ease;
```

### mobile.css 提供的全局类

| 类名                    | 用途                            |
| ----------------------- | ------------------------------- |
| `.hai-safe-top`         | 上方安全区域 padding            |
| `.hai-safe-bottom`      | 下方安全区域 padding            |
| `.hai-safe-all`         | 四周安全区域 padding            |
| `.hai-scroll-container` | 优化的滚动容器（momentum 滚动） |
| `.hai-keyboard-aware`   | 虚拟键盘弹起时自动调整内容      |

## 主题系统

支持 15 个精选 DaisyUI 主题。

```typescript
import { applyTheme, getCurrentTheme, getSavedTheme, isDarkTheme, THEMES, THEME_GROUPS } from '@h-ai/ui/theme'

applyTheme('dark')      // 应用主题（自动持久化 localStorage）
getCurrentTheme()        // 获取当前主题名
getSavedTheme('corporate') // 存储为空或不可用时使用应用默认主题
isDarkTheme('dracula')   // 检查是否暗色主题
```

## 代码高亮（Shiki）

AI 场景组件（`MarkdownRenderer`、`AiDocumentEditor`）内置 Shiki 代码高亮（纯 ESM，支持 Vite SSR/Client 双模式）。

### 支持语言（27 种）

typescript, javascript, python, java, go, rust, c, cpp, csharp, ruby, php, swift, kotlin, sql, html, css, json, yaml, toml, markdown, bash, shell, powershell, dockerfile, xml, graphql, plaintext

### 自定义高亮颜色

通过 CSS 变量覆盖（在 `app.css` 或组件作用域内）：

```css
:root {
  --hai-hl-keyword: #c586c0;
  --hai-hl-string: #ce9178;
  --hai-hl-comment: #6a9955;
  --hai-hl-function: #dcdcaa;
  --hai-hl-variable: #9cdcfe;
  --hai-hl-type: #4ec9b0;
  --hai-hl-number: #b5cea8;
  --hai-hl-operator: #d4d4d4;
  --hai-hl-punctuation: #808080;
  --hai-hl-foreground: #d4d4d4;
  --hai-hl-background: #1e1e1e;
}
```

## 配置驱动认证页面

密码登录只接收 identifier/password；AuthPanel 将 LoginForm 的 username 映射为 identifier。会话期限来自服务端 session.maxAge/refreshTokenMaxAge，不增加未实现的登录偏好字段或持久会话选择框。

协议支持 `agreements.userAgreementMarkdown` 与 `privacyPolicyMarkdown` 直接从配置加载；YAML 的 `|` 保留 Markdown 段落和换行。弹窗复用安全 MarkdownRenderer，关闭原始 HTML、脚本和代码复制工具。同主题同时提供正文与 URL 时优先正文，URL 可作为额外查看入口；只有 URL 时使用隔离 iframe。

```yaml
agreements:
  showOnRegister: true
  showOnLogin: false
  userAgreementMarkdown: |
    # 用户协议

    **账户使用条款**

    - 请妥善保管账户凭据。
  privacyPolicyMarkdown: |
    # 隐私政策

    注册资料用于账户管理，会话信息用于身份认证。
  # 可选 URL；同主题同时配置 Markdown 和 URL 时，弹窗优先显示 Markdown。
  # userAgreementUrl: https://example.com/terms
  # privacyPolicyUrl: https://example.com/privacy
```



AuthPanel 直接接收 IAM 白名单配置和 actions，内置登录（密码/LDAP/邮件/短信）、注册、找回、重置及协议展示。使用 `config.ui` 配置卡片/分栏、标题、品牌、Logo 和配图。`LoginForm` 提供 methods、onsendotp、otpLength、resendInterval；密码表单接受 passwordPolicy。组件只渲染与交互，网络和 Token 存储通过 actions 注入。认证场景按公开 IAM 错误码显示当前 UI 语言的提示，应用自定义错误保留原文。

AuthPanel 在 SSR 阶段禁用原生表单控件，客户端挂载并绑定事件后自动开放输入，避免接管时丢失早到的输入。应用无需额外的就绪适配；E2E 等待控件可编辑，并覆盖客户端脚本延迟加载的场景。

认证链接指向当前完整 URL 时，AuthPanel 只切换场景并阻止重复导航，返回登录后可立即输入。路径、查询串或 hash 不同时交给应用路由处理；Ctrl/Cmd 等修改键点击保留浏览器行为。不要为切换场景额外调用 goto 或 location.reload。


协议在 Modal 内显示并保留表单输入；使用 URL 时，协议页面须允许 iframe 嵌入，组件禁用其脚本，另提供新窗口查看入口。登录/注册表单使用 `header/extraFields/footer` snippet，AuthPanel 透传 `loginHeader/loginFields/loginFooter` 和 `registerHeader/registerFields/registerFooter`，底部保留内置导航。自定义字段由应用绑定状态和提交，不会隐式写入 IAM 用户模型。

```svelte
<!-- config 来自 IAM 公开配置，actions 来自 Kit/API client；m 为应用翻译。 -->
<AuthPanel {config} {actions}>
  {#snippet loginHeader()}
    <p>{m.loginHelp()}</p>
  {/snippet}
  {#snippet registerFields()}
    <label><input type='checkbox' bind:checked={subscribe} />{m.subscribe()}</label>
  {/snippet}
</AuthPanel>
```

上例的 subscribe 为应用自身的 $state，应用包装 register 动作校验/提交扩展业务数据；如需单独组合表单，使用 `<RegisterForm> {#snippet extraFields()}...{/snippet} </RegisterForm>`。扩展里的按钮须为 `type="button"`，避免误提交。


## 重要约定

1. **Svelte 5 Runes**：使用 `$state`、`$derived`、`$effect`
2. **Snippet 插槽**：使用 `{#snippet name()}...{/snippet}` 语法
3. **自动导入例外**：`toast`、类型导入与 `Range` 必须显式 `import`；其余公开 Svelte 组件可自动导入
4. **Combobox 统一单选/多选**：`MultiSelect` 已删除
5. **移动端样式**：务必引入 `design-tokens.css` + `mobile.css`，使用 SafeArea 包裹原生 App 页面
6. **`@source` 必须配置**：未配置则 TailwindCSS 无法扫描 `@h-ai/ui` 组件中的 class，样式会丢失
7. **响应式布局**：后台工作区和数据列表使用可用宽度，避免外壳固定 max-width 造成宽屏两侧留白。flex/grid 子项设 min-width: 0，表格在自身容器滚动；正文和登录框保留阅读宽度。至少验证 390、1440、1920、2560、3840px 的内容占宽、导航与全页溢出。
8. **`ssr.noExternal`**：Svelte 组件及 `@h-ai/kit/client` 的 `$app/*` 路由别名需要 Vite 编译，UI 和 Kit 保持 noExternal，core 及其他纯服务端包保留原生加载；本地链接的框架包也要显式 external，避免日志、数据库等原生依赖被重复打包。sm-crypto 使用动态 CommonJS require，应放入 `ssr.external`，并以明确版本列入应用 dependencies；adapter-node 只根据直接生产依赖排除二次打包，不能仅依赖它作为框架的传递依赖。
9. **data-* 属性透传**：公开 Svelte 组件会把调用方传入的 `data-*` 属性透传到根节点或主交互节点；测试和自动化优先直接使用 `data-testid`、`data-analytics-id`，不要为测试 ID 重复封装组件

## 导出路径

新手引导与帮助中心使用公开 `@h-ai/ui/guide` 入口；`createGuide` 控制器、`GuideHost` 和独立 `HelpCenter` 的接入、首次偏好和导航契约见 [guide.md](guide.md)。

| 路径                        | 用途                                   |
| --------------------------- | -------------------------------------- |
| `@h-ai/ui`                 | 主入口：所有组件、工具函数、类型       |
| `@h-ai/ui/theme`           | 轻量主题元数据与工具，不经过组件根入口 |
| `@h-ai/ui/i18n`            | 轻量语言工具，供状态层使用，不加载组件或引导 CSS |
| `@h-ai/ui/auto-import`     | Svelte 预处理器：组件自动导入          |
| `@h-ai/ui/components/primitives/index` 等明确 barrel | 仅 package.exports 列出的组件目录；组件默认导出经 barrel 转发 |
| `@h-ai/ui/components/primitives/index` | 公开原子组件目录入口，如 Button/Input |
| `@h-ai/ui/styles/global.css`       | 全局基础样式（重置/滚动条/焦点）       |
| `@h-ai/ui/styles/theme.css`        | Tailwind v4 `@theme` Token            |
| `@h-ai/ui/styles/design-tokens.css`| CSS 自定义属性（间距/圆角/z-index）    |
| `@h-ai/ui/styles/mobile.css`       | 移动端优化（安全区域/触摸/键盘）       |

## 相关 Skills

- `hai-kit`：SvelteKit 集成（hooks.server.ts、API 端点、认证守卫）
- `hai-iam`：IAM 模块 API（与 LoginForm/RegisterForm 配合）
- `hai-capacitor`：原生 App 开发（与 SafeArea/AppBar 配合）
- `hai-api-client`：客户端数据获取

## 表单布局与错误提示

数据新增/修改统一使用 `Form`（托管提交）或原生 `<form>` + `FormLayout`，不要在应用复制字段 grid。两者支持 `columns={1 | 2}`、`labelPosition='top' | 'left'`、`labelWidth`；默认单列/标签在上，窄容器自动收拢。`FormField.colSpan='auto'` 自动让多行文本、上传和多选占两列，`1` / `2` 可覆盖；自定义宽控件标注 `data-form-wide`。已有控件 ID 或原生控件给 FormField 配 `for`，避免重复或失效的标签。

IAM 表单、AuthPanel、UserProfile、FeedbackModal 直接接收同组参数；CRUD 使用 `form={{ columns: 2, labelPosition: 'left', labelWidth: '6rem' }}`，字段支持 `colSpan` 和延迟翻译的 `validation.message: () => m.required()`。通用字段的 `label/error/hint`、Input/Textarea/Select 的 `validationMessage` 支持翻译函数。内置必填、类型、长度和范围错误由 UI 提供；应用只提供业务文案。通过 UI 的 `setGlobalLocale` / `createLocaleStore` 同步语言，已显示的错误随之更新；自定义提示只用于失败，不能把固定提示直接设成持续的 customValidity。字段错误只在 FormField 或控件其中一处展示。

验证一列/双列与两种标签位置、自动/显式跨列、窄抽屉/手机、无效提交与修正、切换语言后的错误、按钮和回车提交，以及关闭重开后的状态。


```svelte
<!-- m、save、name、notes 为应用自己的翻译、提交回调和状态 -->
<Form columns={2} labelPosition="left" labelWidth="6rem" onsubmit={save}>
  <FormField label={() => m.name()} required>
    <Input name="name" bind:value={name} required />
  </FormField>
  <FormField label={() => m.notes()}>
    <Textarea name="notes" bind:value={notes} />
  </FormField>
  <Button type="submit">{m.save()}</Button>
</Form>
```

Node 请求/SSR 必须用 `core.i18n.runWithLocale(locale, callback)`（kit 同名方法）包裹实际异步 resolve；serv 自动建立请求语言上下文。浏览器 setGlobalLocale/createLocaleStore 继续管理应用语言，SSR 的 UI setter/store 不写全局值。显式 MessageOptions.locale 优先；进程默认语言仅在启动时设置。

`@h-ai/ui/i18n` 的 `createLocaleStore` 按“可解析的持久化值 → 完整 navigator.languages（再检查 navigator.language）→ defaultLocale → 支持列表首项”选择语言；首个不支持的候选不触发默认回退。匹配大小写、语言简写及逐级区域/脚本回退，返回 supportedLocales 中的配置代码；同语言多个变体按列表顺序选择。detectBrowserLocale 无实际命中返回 null，不插入框架默认值；空支持列表最终保留调用方默认值。SSR 不读取浏览器候选或修改全局语言。应用若使用 Paraglide，仍需用该项目的 setLocale 同步应用翻译；内置 UI 字典的覆盖范围不因自定义 store 列表而自动增加。

## 设计、实现与审查交接

新页面/较大改版由 [hai-ui-design](../hai-ui-design/SKILL.md) 记录任务、信息结构、状态与组件映射；本参考负责公开用法；[hai-ui-review](../hai-ui-review/SKILL.md) 查看真实页面。theme.css 已引入 design-tokens.css；--hai-focus-color 缺省跟随主题 primary。

Tabs 提供单一 Tab 入口、方向键/Home/End、跳过禁用项和 children 内容关联。DataTable 提供 aria-sort、aria-busy、加载文字及行键盘激活。Pagination 导航与输入名称走 UI i18n，外部 page 同步跳页草稿，触摸设备扩大按钮。TableToolbar 进入弹层后聚焦，Escape 返回触发点，视图按钮带 aria-pressed；业务回调仍须实际更新对应数据和视图。Empty/Alert 可容纳长内容与恢复操作；全局样式响应减少动态效果。
