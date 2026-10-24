# 新手引导与帮助中心

`@h-ai/ui/guide` 提供 `createGuide`、`GuideHost`、`HelpCenter` 和相关类型。无需 init/close；driver.js 引擎在开始导览时动态加载，样式由 GuideHost 导入并跟随现有 DaisyUI 主题。

## 最小接入

下面的 `m`、`locale`、`currentUser`、`createUser` 来自应用。翻译 key 需要在应用中英日文消息文件定义；`nav` 使用当前宿主的公开导航适配器。

```svelte
<script lang='ts'>
  import { createGuide, GuideHost } from '@h-ai/ui/guide'
  import { createSvelteKitNavAdapter } from '@h-ai/kit/client'
  import * as m from '$lib/paraglide/messages.js'

  const guide = createGuide({
    nav: createSvelteKitNavAdapter(),
    persistKey: 'my-app-guide',
    tours: [{
      id: 'users',
      version: 1,
      route: '/users',
      title: () => m.guide_users_title(),
      description: () => m.guide_users_description(),
      steps: [{
        target: 'users-create',
        title: () => m.guide_create_title(),
        description: () => m.guide_create_description(),
      }],
    }],
    articles: [{
      id: 'users-help',
      title: () => m.guide_users_title(),
      content: () => m.guide_users_markdown(),
      tourId: 'users',
    }],
  })
</script>

<GuideHost {guide} {locale} scope={currentUser.id} autoStart='users' />
<Button onclick={() => guide.openHelp()}>{m.help()}</Button>
<Button data-guide='users-create' onclick={createUser}>{m.create_user()}</Button>
```

每个布局只放一个宿主；实际目标按钮位于 `route` 对应的页面，跨页面调用时先导航再定位。SPA、Capacitor、Tauri 使用已有 `NavAdapter`；默认导航通过 location.assign 完整加载页面，GuideHost 用短期 sessionStorage 记录在目标页继续启动。

## 行为契约

| API / 配置 | 契约 |
| --- | --- |
| `resolveGuideText(text?)` | 将字符串或 i18n 回调解析为当前文本；宿主组件已自动调用，通常无需手动解析 |
| `guide.start(id)` | 手动启动或重看；返回 `Promise<boolean>`。失败返回 false，`guide.error` 为当前 UI 语言提示 |
| `guide.startIfNeeded(id)` | 尚未看过时提供首次邀请；返回 boolean，不直接跳转。通常由 GuideHost 的 autoStart 代劳 |
| `guide.openHelp(articleId?)` | 打开帮助中心，可直接定位文章；开始导览时自动关闭帮助弹窗 |
| `guide.stop()` | 取消等待并释放浮层，不记为完成；关闭按钮、Escape 和遮罩点击在首帧定位期间也会清除 busy，启动中的 start 返回 false |
| `guide.reset(id)` | 删除当前 scope 的偏好，使首次邀请可以再次显示 |
| `persistence` | `'cookie'`（默认）、`'localStorage'` 或 `'memory'`；受限存储在实例内回退到内存 |
| `persistKey` / `scope` / `version` | 应用、用户、引导版本独立记录；使用非敏感标识，不能用于权限校验 |
| `waitTimeout` | 元素等待上限，默认 3000ms；异步操作取消后不得回填旧状态 |
| `step.before(signal)` | 展开页签或抽屉等 UI 准备，返回 void 或 Promise；不得自动提交业务表单 |
| `step.optional` | 可选元素缺失可跳过；必要元素缺失必须停止，不能误记为完成 |
| `tour.enabled` / `article.enabled` | 依据现有权限或功能开关提供内容，不能代替业务授权 |

首次邀请提供开始、稍后、不再提醒。稍后暂缓一天；完成和不再提醒默认保存一年（Cookie）。关闭导览不等于完成；手动重看始终可用。切换账号或离开当前导览页面会清理交互。

## i18n、样式与边界

- 通用按钮、进度和错误由 UI 内置中英日文维护。业务文案使用 `() => m.key()` 延迟求值；GuideHost 的 locale 接入应用已有语言状态，不另建语言体系。
- 语言变化时宿主重新解析文案并刷新当前浮层。应用若采用整页重载或销毁组件切换语言，应按自身生命周期重新挂载宿主。
- 目标优先使用 `data-guide`，不依赖 Tailwind 类名或 DOM 顺序；现有 UI 组件支持 data 属性透传。复杂目标可用 `target: () => document.querySelector(...)`。
- 步骤标题和说明按纯文本显示；帮助正文使用现有安全 MarkdownRenderer，原始 HTML 关闭。
- 控制器在组件内创建，不能在 SSR 模块级共享用户状态。GuideHost 挂载前不读取浏览器记录，卸载时清理。
- 保留 Tailwind 对 UI 包的 `@source` 扫描。只改变应用局部 data-theme 时，body 浮层需要同步相同主题；标准 applyTheme 设置 html 的 data-theme，浮层会自动继承。
- HelpCenter 可独立作为帮助页面：提供 articles、可选 tours、locale、selectedId 和 onstart，无需自己实现搜索与安全 Markdown 渲染。

## 默认应用

fullstack 的 Web/App/Desktop 在共享 AdminWorkspace 中启用帮助入口、首次邀请和个人设置导览；Web 使用 Cookie，App/Desktop 使用 localStorage，按当前用户 ID 隔离。新增业务功能时，在共享 guide-store 和中英日文消息中补充文章、导览和稳定锚点。

验证首次提示、完成后刷新、手动重看、跳转后等待、缺失目标、账号/版本隔离、语言与主题、移动宽度、键盘和卸载取消。
