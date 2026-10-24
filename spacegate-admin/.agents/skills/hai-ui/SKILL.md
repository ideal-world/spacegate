---
name: hai-ui
description: "使用 @h-ai/ui 构建 Svelte 5 表单、表格、弹窗、新手引导、帮助中心、主题与 AI 内容展示。"
---

# hai-ui

## 能力契约

主题 DOM 与安全缓存复用 `@h-ai/ui/theme` 的 `applyTheme` / `getSavedTheme(fallbackTheme?)`；默认主题取应用配置，选择范围与已编译 CSS 主题一致。

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/ui 构建 Svelte 5 表单、表格、弹窗、移动界面、主题与 AI 内容展示 |
| 适用场景 | 已有 Svelte 项目中的组件选择与交互实现 |
| 输入 | 现有 Svelte 页面、组件 Props、Tailwind 扫描路径与主题 |
| 输出 | 复用原子/组合/场景组件的界面及交互验证 |
| 限制 | 无 init/close；按公开导出使用组件。复用内置翻译；SSR 不直接访问浏览器全局，高风险代码预览须显式启用。 |

## 使用路径

新页面或较大改版先使用 [hai-ui-design](../hai-ui-design/SKILL.md) 确定任务、布局、状态和组件映射；本 skill 负责正确实现。完成后按 [hai-ui-review](../hai-ui-review/SKILL.md) 查看真实页面，小范围修复沿用已有设计。

- 密码登录只接收 identifier/password；AuthPanel 将 LoginForm 的 username 映射为 identifier。会话期限来自服务端 session.maxAge/refreshTokenMaxAge，不增加未实现的登录偏好字段或持久会话选择框。

- 先检查组件导入、Tailwind @source 和主题 CSS，再选场景 → 组合 → 原子组件，缺能力才自定义。
- 管理列表和工作台随可用宽度展开；flex/grid 子项设 min-width: 0，表格在自身容器滚动。阅读页与登录表单按内容约束宽度。实际检查 390、1440、1920、2560、3840px，避免宽屏仍挤在中央窄列，手机出现整页横向滚动。
- 按安装版本的 exports 选择明确列出的 components barrel 入口（不开放任意内部文件）；主题工具使用 `@h-ai/ui/theme`，避免仅为主题加载根入口的 AI/图表组件。禁止导入内部 src/dist；例如原子组件使用 `@h-ai/ui/components/primitives/index`。
- 自动导入识别无分号、跨行及别名的运行时 import；已绑定的本地组件名称不会重复注入。
- 应用只提供页面级文案；组件内部中英日文由 @h-ai/ui 维护。
- `UserProfile` 用 `avatarAccept` / `avatarHint` 对齐业务的头像格式与大小限制；文件最终校验和上传由业务回调完成。
- `BareInput type="file"` 通过 `onchange` 的 `currentTarget.files` 读取文件，不绑定文件路径 `value`；其他输入类型可 `bind:value`。
- AI 文本视为不可信；Markdown/Mermaid 预览与允许执行 HTML/JS 的预览有不同边界。

- Tabs 支持方向键、Home/End、跳过禁用项与单一 Tab 入口，children 内容关联当前页签；DataTable 通过 aria-sort/aria-busy 和加载文字提供状态，有 onrowclick 时支持行 Enter/Space，单元格操作不重复触发行操作。
- Pagination 导航和输入名称跟随 UI locale，外部 page 变化同步跳页草稿，触摸设备扩大导航按钮；TableToolbar 打开后进入控件，Escape 返回触发点，视图按钮带选中状态。应用仍需接入排序、筛选、分页和视图对应的数据行为。
- CrudPage 占满宿主剩余高度，空数据和少量数据时分页保留在底部；宿主从视口外壳到内容区逐层使用 flex/grid 的剩余空间约束与 min-height: 0，避免自动高度包装层截断布局。数据区独立滚动，分页不覆盖数据。
- theme.css 自动引入 design-tokens.css；全局样式的焦点使用 --hai-focus-color（缺省跟随当前 primary），减少动态效果时保留最终状态并缩短动画。长内容在 Empty/Alert 中换行；业务自定义 snippet 也要能收缩。

## 按需参考

新手引导、首次 Cookie 提示、操作导览和帮助中心见 [guide.md](guide.md)。fullstack 默认在共享管理界面接入，新增业务只补内容与 data-guide 锚点。

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用边界
- AI 使用顺序（先判断，再落地）
- 项目配置（从 npm 安装）
- 三层组件架构
- Design Token 系统
- 主题系统
- 代码高亮（Shiki）
- 重要约定
- 导出路径
- 相关 Skills

## 表单布局与错误提示

输入控件与 CRUD 新增/编辑默认在首次失焦后显示前端错误，修正输入时即时更新；未操作字段在提交时校验。Select 在焦点离开触发器及选项后校验。应用无需逐项绑定 blur；业务异步校验仍由应用提供。PasswordInput 的 minLength 提供时执行长度校验；登录现有密码时省略该属性。

UserProfile 的 `fieldConstraints` 按字段声明 `required/minlength/maxlength/pattern/validationMessage`；单行控件支持 pattern，业务文案可用翻译函数。不要只在保存回调内放前端校验，服务端仍须执行 schema 校验。

状态层通过轻量入口 `@h-ai/ui/i18n` 导入 `setGlobalLocale` / `createLocaleStore`，不加载组件或引导 CSS。

数据新增/修改统一使用 `Form`（托管提交）或原生 `<form>` + `FormLayout`，不要在应用复制字段 grid。两者支持 `columns={1 | 2}`、`labelPosition='top' | 'left'`、`labelWidth`；默认单列/标签在上，窄容器自动收拢。`FormField.colSpan='auto'` 自动让多行文本、上传和多选占两列，`1` / `2` 可覆盖；自定义宽控件标注 `data-form-wide`。已有控件 ID 或原生控件给 FormField 配 `for`，避免重复或失效的标签。

IAM 表单、AuthPanel、UserProfile、FeedbackModal 直接接收同组参数；CRUD 使用 `form={{ columns: 2, labelPosition: 'left', labelWidth: '6rem' }}`，字段支持 `colSpan` 和延迟翻译的 `validation.message: () => m.required()`。通用字段的 `label/error/hint`、Input/Textarea/Select 的 `validationMessage` 支持翻译函数。内置必填、类型、长度和范围错误由 UI 提供；应用只提供业务文案。通过 UI 的 `setGlobalLocale` / `createLocaleStore` 同步语言，已显示的错误随之更新；自定义提示只用于失败，不能把固定提示直接设成持续的 customValidity。字段错误只在 FormField 或控件其中一处展示。

验证一列/双列与两种标签位置、自动/显式跨列、窄抽屉/手机、无效提交与修正、切换语言后的错误、按钮和回车提交，以及关闭重开后的状态。

Node 请求/SSR 必须用 `core.i18n.runWithLocale(locale, callback)`（kit 同名方法）包裹实际异步 resolve；serv 自动建立请求语言上下文。浏览器 setGlobalLocale/createLocaleStore 继续管理应用语言，SSR 的 UI setter/store 不写全局值。显式 MessageOptions.locale 优先；进程默认语言仅在启动时设置。

`@h-ai/ui/i18n` 的 `createLocaleStore` 按“可解析的持久化值 → 完整 navigator.languages（再检查 navigator.language）→ defaultLocale → 支持列表首项”选择语言；首个不支持的候选不触发默认回退。匹配大小写、语言简写及逐级区域/脚本回退，返回 supportedLocales 中的配置代码；同语言多个变体按列表顺序选择。detectBrowserLocale 无实际命中返回 null，不插入框架默认值；空支持列表最终保留调用方默认值。SSR 不读取浏览器候选或修改全局语言。应用若使用 Paraglide，仍需用该项目的 setLocale 同步应用翻译；内置 UI 字典的覆盖范围不因自定义 store 列表而自动增加。
