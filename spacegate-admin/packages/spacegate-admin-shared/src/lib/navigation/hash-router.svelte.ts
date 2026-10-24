import { SvelteMap, SvelteURL } from 'svelte/reactivity'

/**
 * 页面目录驱动的 hash 导航；仅管理 URL，不包含业务权限或页面加载逻辑。
 * @example
 * ```ts
 * const pages = import.meta.glob('../pages/*.svelte')
 * const router = new HashRouter(Object.keys(pages), '/home')
 * const stop = router.start(() => void loadCurrentPage())
 * router.navigate('/reports/weekly?period=month')
 * stop()
 * ```
 */
export class HashRouter {
  /** 当前页面路径，例如 `/reports/weekly`。 */
  path = $state('')
  /** 当前页面查询串，例如 `?period=month`。 */
  search = $state('')
  private readonly files = new SvelteMap<string, string>()

  private readonly fallback: string

  /**
   * 建立页面文件到 URL 的映射，新增页面无需维护路由表。
   * @param pageFiles - import.meta.glob 返回的文件名
   * @param fallback - 空路径和未知页面的回退路径
   * @example new HashRouter(Object.keys(import.meta.glob('../pages/*.svelte')), '/home')
   */
  constructor(pageFiles: string[], fallback: string) {
    this.fallback = fallback
    for (const file of pageFiles) {
      const route = file.replace(/^.*\/pages/, '').replace(/\.svelte$/, '').replace(/\/index$/, '') || '/'
      this.files.set(route, file)
    }
    if (!this.files.has(fallback))
      throw new Error('Hash router fallback page is missing')
    this.path = fallback
  }

  /** 当前路径对应的页面文件；可用于索引 import.meta.glob 的惰性加载器。 */
  get file(): string {
    return this.files.get(this.path) ?? this.files.get(this.fallback)!
  }

  /**
   * 恢复地址栏状态并监听链接、前进与后退；卸载时调用返回的清理函数。
   * @param onChange - URL 导航完成后的业务通知
   * @returns 移除事件监听器的函数
   * @example const stop = router.start(() => void store.navigate(router.path.slice(1)))
   */
  start(onChange: () => void = () => {}): () => void {
    const synchronize = () => {
      this.navigate(location.hash.slice(1), true)
      onChange()
    }
    synchronize()
    window.addEventListener('hashchange', synchronize)
    return () => {
      window.removeEventListener('hashchange', synchronize)
    }
  }

  /**
   * 导航到页面；未知路径回退，查询串随 hash 保留并可在刷新后恢复。
   * @param target - 以 `/` 开头的本地页面路径和查询串
   * @param replace - 是否替换当前历史项
   * @example router.navigate('/settings?section=profile')
   */
  navigate(target: string, replace = false): void {
    const local = target.startsWith('/') && !target.startsWith('//') && !target.includes('\\')
    const url = new SvelteURL(local ? target : this.fallback, 'https://hash.invalid')
    const known = this.files.has(url.pathname)
    this.path = known ? url.pathname : this.fallback
    this.search = known ? url.search : ''
    const hash = `#${this.path}${this.search}`
    if (location.hash !== hash) {
      if (replace)
        history.replaceState(history.state, '', hash)
      else
        history.pushState(history.state, '', hash)
    }
  }
}
