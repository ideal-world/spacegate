---
name: hai-capacitor
description: "在 Android/iOS 应用中接入 @h-ai/capacitor 设备、相机、推送与安全 Token 存储，处理 Web/原生差异。"
---

# hai-capacitor

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 使用 @h-ai/capacitor 接入原生设备、相机、推送、状态栏和安全 Token 存储 |
| 适用场景 | Android/iOS 原生壳集成与平台差异处理 |
| 输入 | Capacitor 配置、原生插件、设备权限和平台状态 |
| 输出 | 原生操作结果及 TokenStorage 适配器 |
| 限制 | 区分浏览器与 Android/iOS；安全存储需原生插件，Web 使用 httpOnly Cookie/内存认证，不降级到 localStorage。H5 验证不能代替真机验证。 |

## 使用路径

- 先判断平台和插件可用性，复用已有 SPA 入口；原生 UI 变化验证权限拒绝、取消和返回路径。
- TokenStorage 在原生端使用安全插件；Web/插件不可用或存储失败时拒绝 Promise。调用方处理 setTokens/clear 失败并释放 loading；clear 的实例内屏蔽不替代服务端撤销。
- 原生打包前确认公开 API 地址的运行配置入口；fullstack App 使用 `public/runtime-config.js`，构建不会读取部署环境文件。这里只放公开地址，不能放 Token 或密钥。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 运行环境
- 适用场景
- 使用步骤
- 核心 API
- 错误码 — `HaiCapacitorError`
- 常见模式
- 插件依赖
- 相关 Skills

推送仅在实际原生平台调用插件。`push.register({ signal? })` 与 `push.listen(callbacks, { signal? })` 的设置阶段限时 30 秒（包含权限和监听器挂载）；失败、超时、取消及 `capacitor.close()` 都会停止回调并移除本次拥有的句柄，迟到的句柄在完成后立即移除，不调用影响其他消费者的 removeAllListeners。listen 成功后信号仍有效，也可调用返回的 cleanup。cleanup 与 close 均返回 `HaiResult<void>`；原生 remove 失败返回 `PUSH_CLEANUP_FAILED`（`capacitor:032`），失败句柄保留供再次调用重试，成功移除的句柄不重复操作。模块关闭后立即不可用；重新初始化前先重试遗留清理。自动化验证控制插件时序，不能代替 Android/iOS 真机 FCM/APNs 权限、注册与反复挂载验收。
