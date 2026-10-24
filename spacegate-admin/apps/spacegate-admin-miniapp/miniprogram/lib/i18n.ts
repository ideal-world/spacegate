import { messages } from '../i18n/messages'

/** 小程序支持的界面语言；与共享翻译 locale 保持一致。 */
export type Locale = 'zh-CN' | 'en-US' | 'ja-JP'

const FALLBACK_LOCALE: Locale = 'zh-CN'
const SUPPORTED_LOCALES: readonly Locale[] = ['zh-CN', 'en-US', 'ja-JP']

let currentLocale: Locale = FALLBACK_LOCALE

/** 判断任意字符串是否为受支持语言。 */
export function isLocale(value: string): value is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(value)
}

/**
 * 切换当前界面语言；未知语言回退到 zh-CN。
 * @param value - 语言代码
 * @example
 * ```ts
 * setLocale('en-US')
 * t('save') // 'Save'
 * ```
 */
export function setLocale(value: string): void {
  currentLocale = isLocale(value) ? value : FALLBACK_LOCALE
}

/** 当前界面语言。 */
export function getLocale(): Locale {
  return currentLocale
}

/** 可供 picker 使用的语言列表及展示名。 */
export function localeOptions(): { value: Locale, label: string }[] {
  return SUPPORTED_LOCALES.map(locale => ({ value: locale, label: locale === 'zh-CN' ? '简体中文' : locale === 'en-US' ? 'English' : '日本語' }))
}

/**
 * 读取翻译并替换 `{name}` 占位符。
 *
 * 查找顺序：当前语言 -> zh-CN -> 原始 key，确保后端新增 key 未翻译时仍可读。
 * @param key - 翻译键
 * @param params - 占位符参数
 * @returns 翻译文本；缺失时返回 key 本身
 * @example
 * ```ts
 * t('mini_health_summary', { online: 2, total: 3 }) // '在线 2 / 共 3'
 * ```
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const template = messages[currentLocale]?.[key] ?? messages[FALLBACK_LOCALE]?.[key] ?? key
  if (!params)
    return template
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name]
    return value === undefined ? match : String(value)
  })
}

/**
 * 批量生成页面标签对象，避免在 WXML 中调用函数。
 * @param keys - 页面使用的翻译键
 * @returns 以 key 为属性的翻译对象
 * @example
 * ```ts
 * const labels = labelsFor(['save', 'cancel'])
 * labels.save // '保存'
 * ```
 */
export function labelsFor<K extends string>(keys: readonly K[]): Record<K, string> {
  const result = {} as Record<K, string>
  for (const key of keys)
    result[key] = t(key)
  return result
}
