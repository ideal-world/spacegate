/**
 * 进程内界面状态：只保存当前网关名称与语言。
 * 不保存 access/refresh token；小程序进程重启后必须重新登录。
 */
let currentGateway = ''

/** 当前操作的网关名；为空表示尚未从配置列表选择。 */
export function getCurrentGateway(): string {
  return currentGateway
}

/** 更新当前网关；只影响界面，不写入 storage。 */
export function setCurrentGateway(name: string): void {
  currentGateway = name
}
