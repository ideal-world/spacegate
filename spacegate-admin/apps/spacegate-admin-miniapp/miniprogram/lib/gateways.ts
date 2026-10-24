import { spacegate } from './api'
import { getCurrentGateway, setCurrentGateway } from './state'

/** 网关列表选择结果；`version` 用于需要写操作的页面。 */
export interface GatewaySelection {
  names: string[]
  current: string
  version: number
}

/**
 * 拉取网关名列表并补齐当前选择。
 *
 * 优先保留用户上次选择；选择已失效时回退到第一个网关。空列表返回空字符串。
 * @returns 名称列表、当前选择和服务端配置版本
 * @example
 * ```ts
 * const { names, current, version } = await loadGatewaySelection()
 * ```
 */
export async function loadGatewaySelection(): Promise<GatewaySelection> {
  const result = await spacegate.config.names()
  const names = [...result.items]
  const existing = getCurrentGateway()
  const current = existing && names.includes(existing) ? existing : (names[0] ?? '')
  setCurrentGateway(current)
  return { names, current, version: result.version }
}

/** 把 picker 的索引转换为网关名。 */
export function gatewayAt(names: readonly string[], index: number): string {
  return names[index] ?? ''
}
