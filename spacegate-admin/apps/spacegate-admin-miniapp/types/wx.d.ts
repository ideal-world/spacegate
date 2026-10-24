/**
 * 微信小程序运行时的最小本地类型声明。
 *
 * 工程不引入第三方 miniprogram 类型包，只声明当前页面和适配器实际使用的 API，
 * 因此类型检查可在 Node/vitest 中直接运行。协议细节以微信官方文档为准。
 */

/** `wx.request` 成功回调结构。 */
interface WxRequestSuccess<T = unknown> {
  readonly data: T
  readonly statusCode: number
  readonly header: Record<string, string>
}

/** `wx.request` 失败回调结构。 */
interface WxRequestFail {
  readonly errMsg: string
}

/** `wx.request` 返回的可中断任务。 */
interface WxRequestTask {
  abort: () => void
}

/** `wx.request` 调用参数。 */
interface WxRequestOptions<T = unknown> {
  url: string
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'HEAD' | 'OPTIONS'
  data?: unknown
  header?: Record<string, string>
  timeout?: number
  dataType?: 'json' | 'text'
  success?: (result: WxRequestSuccess<T>) => void
  fail?: (error: WxRequestFail) => void
  complete?: () => void
}

/** `wx.showModal` 成功回调。 */
interface WxShowModalResult {
  readonly confirm: boolean
  readonly cancel: boolean
  readonly content?: string
}

/** 应用级全局数据；token 不在其中，只保留非敏感界面状态。 */
interface WxAppGlobalData {
  locale: string
  currentGateway: string
}

/** 小程序 API 的最小集合；测试 mock 只需实现实际调用的方法。 */
interface WxApi {
  request: <T = unknown>(options: WxRequestOptions<T>) => WxRequestTask
  getExtConfigSync?: () => Record<string, unknown>
  showToast?: (options: { title: string, icon?: 'none' | 'success' | 'error' | 'loading', duration?: number }) => void
  showModal?: (options: {
    title?: string
    content?: string
    editable?: boolean
    placeholderText?: string
    confirmText?: string
    cancelText?: string
    success?: (result: WxShowModalResult) => void
    fail?: (error: WxRequestFail) => void
  }) => void
  showLoading?: (options: { title: string, mask?: boolean }) => void
  hideLoading?: () => void
  setNavigationBarTitle?: (options: { title: string }) => void
  navigateTo?: (options: { url: string }) => void
  redirectTo?: (options: { url: string }) => void
  reLaunch?: (options: { url: string }) => void
  navigateBack?: (options?: { delta?: number }) => void
  getStorageSync?: (key: string) => unknown
  setStorageSync?: (key: string, value: unknown) => void
  removeStorageSync?: (key: string) => void
}

declare var wx: WxApi

declare function getApp<T = { globalData: WxAppGlobalData }>(): T

/** 页面实例的最小类型；`setData` 只接受页面 `data` 的子集。 */
interface WxPageInstance<TData extends Record<string, unknown>> {
  data: TData
  setData: (patch: Partial<TData>) => void
}

/**
 * 注册页面并保留 `this` 的页面实例类型。
 * @example
 * ```ts
 * Page({ data: { title: '' }, onLoad() { this.setData({ title: 'x' }) } })
 * ```
 */
declare function Page<TOptions extends { data: Record<string, unknown> }>(
  options: TOptions & ThisType<WxPageInstance<TOptions['data']> & TOptions>,
): void

/**
 * 注册小程序应用。
 * @example
 * ```ts
 * App({ globalData: { locale: 'zh-CN', currentGateway: '' } })
 * ```
 */
declare function App<TOptions extends Record<string, unknown>>(options: TOptions & ThisType<TOptions>): void

/** 表单输入事件。 */
interface WxInputEvent {
  readonly detail: { readonly value: string }
  readonly currentTarget: { readonly dataset: Record<string, unknown> }
}

/** 按钮/列表点击事件；`dataset` 值来自 WXML `data-*` 属性。 */
interface WxTapEvent {
  readonly currentTarget: { readonly dataset: Record<string, unknown> }
}

/** picker 选择事件。 */
interface WxPickerEvent {
  readonly detail: { readonly value: string | number }
}
