import { ApiError, ConflictError, ForbiddenError, NetworkError, SessionExpiredError, UnauthorizedError, ValidationError } from './api'
import { t } from './i18n'

/** 页面本地校验错误：直接展示具体字段提示。 */
export class FormError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'FormError'
  }
}

/** 将任意异常转换为可展示的本地化文案；不包含 token 或服务端堆栈。 */
export function describeError(error: unknown): string {
  if (error instanceof FormError)
    return error.message
  if (error instanceof SessionExpiredError || error instanceof UnauthorizedError)
    return t('session_expired')
  if (error instanceof ConflictError) {
    return error.serverVersion === undefined
      ? t('mini_conflict_unknown')
      : t('mini_conflict_reload', { version: error.serverVersion })
  }
  if (error instanceof ForbiddenError)
    return t('forbidden')
  if (error instanceof NetworkError)
    return t('network_error')
  if (error instanceof ValidationError)
    return t('validation_failed')
  if (error instanceof ApiError && error.httpStatus === 403)
    return t('forbidden')
  return t('operation_failed')
}

/** 轻提示；仅用于一次性反馈。 */
export function toast(message: string): void {
  if (typeof wx !== 'undefined' && typeof wx.showToast === 'function')
    wx.showToast({ title: message, icon: 'none', duration: 2400 })
}

/** 展示错误提示。 */
export function toastError(error: unknown): string {
  const message = describeError(error)
  toast(message)
  return message
}

/** 展示成功提示。 */
export function toastDone(message = t('mini_operation_done')): void {
  toast(message)
}

/** 需要用户确认的删除操作。 */
export function confirmDelete(name: string): Promise<boolean> {
  return confirmAction(t('mini_confirm_delete'), `${name}\n${t('mini_delete_warning')}`)
}

/** 通用确认弹窗；确认返回 true。 */
export function confirmAction(title: string, content: string, confirmText = t('delete')): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    if (typeof wx === 'undefined' || typeof wx.showModal !== 'function') {
      resolve(false)
      return
    }
    wx.showModal({
      title,
      content,
      confirmText,
      cancelText: t('cancel'),
      success: result => resolve(result.confirm),
      fail: () => resolve(false),
    })
  })
}

/** 解析 JSON 文本为对象；错误信息包含字段标签但不回显原文。 */
export function parseJsonObject(text: string, label: string): Record<string, unknown> {
  let value: unknown
  try {
    value = JSON.parse(text)
  }
  catch {
    throw new FormError(`${label}: ${t('mini_json_invalid')}`)
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value))
    throw new FormError(`${label}: ${t('mini_json_object_required')}`)
  return value as Record<string, unknown>
}

/** 解析任意 JSON 值；插件 spec 允许对象、数组或标量，不做对象限制。 */
export function parseJsonValue(text: string, label: string): unknown {
  try {
    return JSON.parse(text)
  }
  catch {
    throw new FormError(`${label}: ${t('mini_json_invalid')}`)
  }
}

/** 解析 JSON 数组；用于路由 plugins 绑定。 */
export function parseJsonArray(text: string, label: string): unknown[] {
  let value: unknown
  try {
    value = JSON.parse(text)
  }
  catch {
    throw new FormError(`${label}: ${t('mini_json_invalid')}`)
  }
  if (!Array.isArray(value))
    throw new FormError(`${label}: ${t('mini_json_array_required')}`)
  return value
}

/** 序列化任意值用于展示；循环/undefined 安全返回 'null'。 */
export function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value ?? null, null, 2) ?? 'null'
  }
  catch {
    return 'null'
  }
}

/** 单行摘要，用于列表预览；超长截断，避免渲染整份配置。 */
export function shortJson(value: unknown, limit = 120): string {
  let text: string
  try {
    text = JSON.stringify(value ?? null) ?? 'null'
  }
  catch {
    return 'null'
  }
  return text.length > limit ? `${text.slice(0, limit)}…` : text
}

/** 从详情对象读取字符串字段，缺失时返回空串。 */
export function readString(value: unknown, key: string): string {
  if (typeof value !== 'object' || value === null)
    return ''
  const candidate = (value as Record<string, unknown>)[key]
  return typeof candidate === 'string' ? candidate : ''
}

/** 读取 dataset 中的字符串值。 */
export function datasetString(event: WxTapEvent, key: string): string {
  const value = event.currentTarget.dataset[key]
  return typeof value === 'string' ? value : ''
}

/** 读取 dataset 中的数字索引。 */
export function datasetIndex(event: WxTapEvent, key: string): number {
  const value = event.currentTarget.dataset[key]
  const index = typeof value === 'number' ? value : Number(value)
  return Number.isInteger(index) ? index : -1
}
