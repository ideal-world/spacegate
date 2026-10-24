# hai-capacitor — 详细用法

从 [SKILL.md](SKILL.md) 选择主题后读取对应小节。代码片段展示 API 用法；省略的业务变量、schema、依赖与 return 上下文需由应用补齐。

> `@h-ai/capacitor` 是 hai-framework 的 Capacitor 原生桥接模块。设备、推送等领域操作返回 `HaiResult<T>`；TokenStorage 是直接 Promise 适配器，失败会拒绝。与 `@h-ai/api-client` 配合时，Token 仅存于原生安全存储，不回退到 Web `localStorage`。

## 运行环境

> **浏览器端 / 原生 App 专用。** 在 Capacitor 原生环境（Android/iOS）中提供完整能力；`createCapacitorTokenStorage()` 仅在原生环境持久化 token，纯 Web 不做不安全回退。`capacitor.preferences` 仍可用于普通偏好数据。

## 适用场景

- Android/iOS 原生应用开发（Svelte 5 + Vite + Capacitor）
- Token 安全存储（`@aparajita/capacitor-secure-storage`）
- 设备信息获取（平台、型号、版本）
- 推送通知注册与监听（FCM / APNs）
- 原生相机拍照 / 相册选取
- 状态栏配置（沉浸式、颜色、样式）

## 使用步骤

### 1. 初始化与关闭

```typescript
import { capacitor } from '@h-ai/capacitor'

// 应用启动时初始化（检测 Capacitor 环境可用性）
const result = await capacitor.init()
if (!result.success) {
  // result.error.code === HaiCapacitorError.NOT_AVAILABLE.code
  // 非 Capacitor 环境（纯 Web）
}

// 检查状态
capacitor.isInitialized // boolean
capacitor.getPlatform() // 'android' | 'ios' | 'web'
capacitor.isNative()    // true = 原生 App

// 关闭模块（重置状态）
await capacitor.close()
```

### 2. Token 安全存储（与 api-client 配合）

先复用应用已有的 API 初始化入口。示例中的 `apiBase` 由应用解析公开运行配置，已经包含 API 前缀，不重复拼接 `/api/v1`。Web 部署可通过 `runtime-config.js` 覆盖地址；原生包在复制静态资源前设置公开配置。

```typescript
import { apiClient } from '@h-ai/api-client'
import { createCapacitorTokenStorage } from '@h-ai/capacitor'

await apiClient.init({
  baseUrl: apiBase,
  auth: {
    storage: createCapacitorTokenStorage(),
  },
})
```

`createCapacitorTokenStorage()` 返回 `TokenStorage` 实例（兼容 `@h-ai/api-client`），底层使用 `@aparajita/capacitor-secure-storage`：

- Android → Android KeyStore + 加密 SharedPreferences
- iOS → Keychain
- Web → **不提供原生存储回退**；适配器方法拒绝 Promise，Web 入口选择 api-client 的 Cookie/内存存储

读取、保存与删除失败均拒绝 Promise。调用方通过 `apiClient.auth` 串行更新凭据并捕获 setTokens/clear 失败；界面释放 loading，退出失败仍清理内存身份并提示错误。`clear()` 开始立即屏蔽本实例两类旧凭据，等待两个删除都结束；即使失败也保持屏蔽，相应 key 成功写入后重新开放。屏蔽不等于持久删除，跨实例/重启撤销仍依赖服务端退出/撤销会话。插件恢复后可以在同一实例重试，不缓存永久失败状态。

任一 key 写入失败也会屏蔽整对凭据，避免 access 已落盘而 refresh 保存失败后的半完成登录。登录/注册保存失败后尝试 `auth.clear()` 清理部分数据；清理失败继续显示未确认删除的错误，不能只清界面身份或宣称持久删除成功。

#### Preferences 子操作

```typescript
import { capacitor } from '@h-ai/capacitor'

// 返回 HaiResult<string | null>
const result = await capacitor.preferences.get('my_key')
if (result.success) {
  // result.data — 值或 null
}

await capacitor.preferences.set('my_key', 'value')
await capacitor.preferences.remove('my_key')
```

### 3. 设备信息

```typescript
import { capacitor } from '@h-ai/capacitor'

const info = await capacitor.device.getInfo()
if (info.success) {
  info.data.platform    // 'android' | 'ios' | 'web'
  info.data.model       // 'Pixel 7'
  info.data.osVersion   // '14'
  info.data.manufacturer // 'Google'
  info.data.isVirtual   // false
}

const version = await capacitor.device.getAppVersion()
if (version.success) {
  version.data.version // '1.0.0'
  version.data.build   // '42'
}
```

### 4. 推送通知

```typescript
import { capacitor } from '@h-ai/capacitor'

// 注册推送（请求权限 + 获取设备 Token）
const reg = await capacitor.push.register()
if (reg.success) {
  // 将 reg.data.token 上报给后端
  await api.post('/push/register', { token: reg.data.token })
}

// 监听推送事件（返回 HaiResult 包裹的 async 清理函数）
const listenResult = await capacitor.push.listen({
  onReceived: (notification) => {
    // 前台收到推送
    // notification: { id, title?, body?, data? }
  },
  onActionPerformed: (notification) => {
    // 用户点击推送
  },
})

// 停止监听
if (listenResult.success) {
  await listenResult.data()
}
```

### 5. 相机

```typescript
import { capacitor } from '@h-ai/capacitor'

const photo = await capacitor.camera.takePhoto({
  quality: 80,           // 0-100
  source: 'camera',      // 'camera' | 'photos' | 'prompt'
  resultType: 'base64',  // 'uri' | 'base64' | 'dataUrl'
  width: 800,            // 最大宽度（可选）
  height: 600,           // 最大高度（可选）
})

if (photo.success) {
  const imgSrc = `data:image/${photo.data.format};base64,${photo.data.data}`
}
```

### 6. 状态栏

```typescript
import { capacitor } from '@h-ai/capacitor'

await capacitor.statusBar.configure({
  backgroundColor: '#ffffff',
  style: 'dark',    // 'dark' | 'light' | 'default'（文字颜色）
  overlay: true,     // 沉浸式
})

await capacitor.statusBar.hide()
await capacitor.statusBar.show()
```

## 核心 API

| API | 用途 | 返回值 |
| --- | --- | --- |
| `capacitor.init()` | 初始化模块（检测环境） | `HaiResult<void>` |
| `capacitor.close()` | 关闭模块并回收推送，失败可重试 | `Promise<HaiResult<void>>` |
| `capacitor.getPlatform()` | 获取当前平台 | `'android' \| 'ios' \| 'web'` |
| `capacitor.isNative()` | 是否为原生环境 | `boolean` |
| `capacitor.isInitialized` | 是否已初始化 | `boolean` |
| `createCapacitorTokenStorage()` | 创建 Token 存储 | `TokenStorage`（兼容 api-client） |
| `capacitor.preferences.get(key)` | 安全读取 Preference | `HaiResult<string \| null>` |
| `capacitor.preferences.set(key, value)` | 安全写入 Preference | `HaiResult<void>` |
| `capacitor.preferences.remove(key)` | 安全删除 Preference | `HaiResult<void>` |
| `capacitor.device.getInfo()` | 设备信息 | `HaiResult<DeviceInfo>` |
| `capacitor.device.getAppVersion()` | 应用版本 | `HaiResult<{ version, build }>` |
| `capacitor.push.register()` | 注册推送 | `HaiResult<PushRegistration>` |
| `capacitor.push.listen(callbacks)` | 监听推送事件 | `HaiResult<PushCleanup>`（cleanup 返回 `Promise<HaiResult<void>>`） |
| `capacitor.camera.takePhoto(options?)` | 拍照 / 选取图片 | `HaiResult<PhotoResult>` |
| `capacitor.statusBar.configure(config)` | 配置状态栏 | `HaiResult<void>` |
| `capacitor.statusBar.show()` | 显示状态栏 | `HaiResult<void>` |
| `capacitor.statusBar.hide()` | 隐藏状态栏 | `HaiResult<void>` |

## 错误码 — `HaiCapacitorError`

| 错误码                                       | code                   | 说明                 |
| -------------------------------------------- | ---------------------- | -------------------- |
| `HaiCapacitorError.INIT_FAILED`              | `hai:capacitor:001`    | 初始化失败           |
| `HaiCapacitorError.NOT_AVAILABLE`            | `hai:capacitor:002`    | Capacitor 不可用     |
| `HaiCapacitorError.INIT_IN_PROGRESS`         | `hai:capacitor:003`    | 正在初始化中         |
| `HaiCapacitorError.NOT_INITIALIZED`          | `hai:capacitor:010`    | 模块未初始化         |
| `HaiCapacitorError.PREFERENCES_GET_FAILED`   | `hai:capacitor:011`    | Preferences 读取失败 |
| `HaiCapacitorError.PREFERENCES_SET_FAILED`   | `hai:capacitor:012`    | Preferences 写入失败 |
| `HaiCapacitorError.PREFERENCES_REMOVE_FAILED`| `hai:capacitor:013`    | Preferences 删除失败 |
| `HaiCapacitorError.DEVICE_INFO_FAILED`       | `hai:capacitor:020`    | 获取设备信息失败     |
| `HaiCapacitorError.APP_VERSION_FAILED`       | `hai:capacitor:021`    | 获取应用版本失败     |
| `HaiCapacitorError.PUSH_REGISTER_FAILED`     | `hai:capacitor:030`    | 推送注册失败         |
| `HaiCapacitorError.PUSH_LISTEN_FAILED`       | `hai:capacitor:031`    | 推送监听失败         |
| `HaiCapacitorError.CAMERA_FAILED`            | `hai:capacitor:040`    | 拍照/相册失败        |
| `HaiCapacitorError.STATUS_BAR_FAILED`        | `hai:capacitor:050`    | 状态栏配置失败       |

## 常见模式

### Mobile App 标准初始化

```typescript
// src/lib/capacitor.ts
import { capacitor } from '@h-ai/capacitor'

export async function initCapacitor() {
  const result = await capacitor.init()
  if (!result.success) {
    return
  }

  if (capacitor.isNative()) {
    await capacitor.statusBar.configure({
      backgroundColor: '#ffffff',
      style: 'light',
      overlay: false,
    })
  }
}
```

```svelte
<!-- src/App.svelte -->
<script lang='ts'>
  import { onMount } from 'svelte'
  import { initCapacitor } from './lib/capacitor'

  onMount(() => { initCapacitor() })
</script>
```

### SPA 模式配置（必需）

Capacitor 原生壳读取 `webDir` 指定的静态产物。纯 Vite 模板默认为 `dist`；SvelteKit 项目需配置静态适配器，输出可能是 `build`。构建器、容器与 `webDir` 必须一致，以下是 Vite 示例：

```typescript
// capacitor.config.ts
import type { CapacitorConfig } from '@capacitor/cli'

const config: CapacitorConfig = {
  appId: 'com.example.app',
  appName: 'Example App',
  webDir: 'dist',
}

export default config
```

### Token 存储 + 认证流程

```typescript
// src/lib/api.ts
import { apiClient } from '@h-ai/api-client'
import { createCapacitorTokenStorage } from '@h-ai/capacitor'

export async function initApi(apiBase: string) {
  return apiClient.init({
    baseUrl: apiBase,
    auth: {
      storage: createCapacitorTokenStorage(),
    },
  })
}

export { apiClient }
```

### 推送通知完整流程

```typescript
import { capacitor } from '@h-ai/capacitor'

export async function setupPush() {
  if (!capacitor.isNative()) {
    return
  }

  const reg = await capacitor.push.register()
  if (!reg.success) {
    return
  }

  // 上报 Token 给后端
  await api.post('/push/register', {
    token: reg.data.token,
    platform: capacitor.getPlatform(),
  })

  // 监听推送
  const listenResult = await capacitor.push.listen({
    onReceived: (n) => {
      // 前台通知处理
    },
    onActionPerformed: (n) => {
      // 用户点击跳转
    },
  })
}
```

### 拍照上传

```typescript
import { capacitor } from '@h-ai/capacitor'

async function captureAndUpload() {
  const photo = await capacitor.camera.takePhoto({
    source: 'camera',
    resultType: 'base64',
    quality: 80,
    width: 1024,
  })

  if (!photo.success) {
    return
  }

  // 上传 base64 给后端
  await api.post('/files/upload', {
    data: photo.data.data,
    format: photo.data.format,
  })
}
```

## 插件依赖

| 插件 | 类型 | 用于 |
| --- | --- | --- |
| `@capacitor/core` | peerDependency（必需） | 核心运行时 |
| `@aparajita/capacitor-secure-storage` | 可选 peerDependency（原生 token 存储必需） | `createCapacitorTokenStorage()` |
| `@capacitor/preferences` | peerDependency（必需） | `capacitor.preferences.*` 普通偏好数据 |
| `@capacitor/device` | 可选 | `getDeviceInfo()` |
| `@capacitor/app` | 可选 | `getAppVersion()` |
| `@capacitor/push-notifications` | 可选 | `registerPush()` / `listenPush()` |
| `@capacitor/camera` | 可选 | `takePhoto()` |
| `@capacitor/status-bar` | 可选 | `configureStatusBar()` / `show/hide` |

可选插件未安装时，对应 API 调用会返回 `err`（动态 import 失败被 catch）。

## 相关 Skills

- `hai-api-client`：HTTP 客户端（Token 管理依赖 capacitor 存储）
- `hai-iam`：认证流程（登录获取 Token → 存储到 Capacitor）
- `hai-ui`：移动端 UI 组件（SafeArea、BottomNav 等）

原生交付必须单独执行 Tauri/Gradle/Xcode 打包，并在目标 OS/设备验证安装启动、登录/退出、重启会话、安全 TokenStore、真实 API 域名与覆盖升级。Web 构建/E2E 不代替原生验收。保存提交、包 hash、平台/工具链及逐项结果；缺平台写未验证，禁止静默 skip。hai-framework 仓库参考 `.github/workflows/native.yml` 与 desktop-app / mobile-app README 的「原生交付与验收」章节。

推送仅在实际原生平台调用插件。`push.register({ signal? })` 与 `push.listen(callbacks, { signal? })` 的设置阶段限时 30 秒（包含权限和监听器挂载）；失败、超时、取消及 `capacitor.close()` 都会停止回调并移除本次拥有的句柄，迟到的句柄在完成后立即移除，不调用影响其他消费者的 removeAllListeners。listen 成功后信号仍有效，也可调用返回的 cleanup。cleanup 与 close 均返回 `HaiResult<void>`；原生 remove 失败返回 `PUSH_CLEANUP_FAILED`（`capacitor:032`），失败句柄保留供再次调用重试，成功移除的句柄不重复操作。模块关闭后立即不可用；重新初始化前先重试遗留清理。自动化验证控制插件时序，不能代替 Android/iOS 真机 FCM/APNs 权限、注册与反复挂载验收。
