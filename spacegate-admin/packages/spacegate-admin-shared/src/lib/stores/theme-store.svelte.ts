import { applyTheme, getSavedTheme } from '@h-ai/ui/theme'

export const DEFAULT_THEME = 'sg-soft'
/** 只维护响应式状态和已编译主题白名单，DOM 与安全持久化复用 UI 公共工具。 */
export const AVAILABLE_THEMES: readonly string[] = ['sg-soft', 'sg-night']

/** 只维护响应式状态和已编译主题白名单，DOM 与安全持久化复用 UI 公共工具。 */
class ThemeState {
  current = $state<string>(DEFAULT_THEME)

  constructor() {
    const saved = getSavedTheme(DEFAULT_THEME)
    this.set(AVAILABLE_THEMES.includes(saved) ? saved : DEFAULT_THEME)
  }

  set(value: string): void {
    if (!AVAILABLE_THEMES.includes(value))
      return
    this.current = value
    applyTheme(value)
  }
}

export const themeStore = new ThemeState()
