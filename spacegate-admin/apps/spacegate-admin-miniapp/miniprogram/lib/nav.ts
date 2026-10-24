import { hasSession } from './api'

/** 小程序页面注册路径；导航统一从这里取，避免散落字符串。 */
export const PAGE_PATHS = {
  login: '/pages/login/index',
  overview: '/pages/overview/index',
  gateways: '/pages/gateways/index',
  routes: '/pages/routes/index',
  plugins: '/pages/plugins/index',
  aiQueue: '/pages/ai-queue/index',
  instances: '/pages/instances/index',
  profile: '/pages/profile/index',
} as const

export type PagePath = typeof PAGE_PATHS[keyof typeof PAGE_PATHS]

/** 打开页面（保留返回栈）。 */
export function openPage(path: PagePath): void {
  if (typeof wx !== 'undefined' && typeof wx.navigateTo === 'function')
    wx.navigateTo({ url: path })
}

/** 重置到登录页；用于登录成功或会话失效。 */
export function goLogin(): void {
  if (typeof wx !== 'undefined' && typeof wx.reLaunch === 'function')
    wx.reLaunch({ url: PAGE_PATHS.login })
}

/** 重置到概览；用于登录成功。 */
export function goOverview(): void {
  if (typeof wx !== 'undefined' && typeof wx.reLaunch === 'function')
    wx.reLaunch({ url: PAGE_PATHS.overview })
}

/** 回到概览；销毁中间页面栈，避免表单草稿残留。 */
export function backToOverview(): void {
  if (typeof wx !== 'undefined' && typeof wx.reLaunch === 'function')
    wx.reLaunch({ url: PAGE_PATHS.overview })
}

/**
 * 页面守卫：内存 token 缺失时跳回登录页。
 * @returns 当前是否可继续加载数据
 */
export function ensureSession(): boolean {
  if (hasSession())
    return true
  goLogin()
  return false
}

/** 设置导航栏标题。 */
export function setPageTitle(title: string): void {
  if (typeof wx !== 'undefined' && typeof wx.setNavigationBarTitle === 'function')
    wx.setNavigationBarTitle({ title })
}
