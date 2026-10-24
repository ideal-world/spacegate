import type { CoreConfig } from '@h-ai/core'
import type { ServConfigInput, ServNodeServer } from '@h-ai/serv'
import process from 'node:process'
import { core } from '@h-ai/core'
import { serv, ServConfigSchema } from '@h-ai/serv'
import { createMiniappServerApp, createServerApp, MINIAPP_API_PREFIX } from './app-server.js'
import { closeModules, initializeModules } from './server/runtime.js'

/**
 * 解析可选的小程序通道端口；未设置时返回 undefined，非法值直接失败。
 * @param value - `HAI_SERV_MINIAPP_PORT` 原始值
 * @returns 1–65535 的整数端口；未设置或空串时为 undefined
 * @throws 值不是合法端口时抛出，避免静默使用错误端口或意外关闭通道
 * @example
 * ```ts
 * parseMiniappPort('3001') // 3001
 * parseMiniappPort(undefined) // undefined
 * ```
 */
function parseMiniappPort(value: string | undefined): number | undefined {
  const trimmed = value?.trim()
  if (!trimmed)
    return undefined
  const port = Number(trimmed)
  if (!Number.isInteger(port) || port < 1 || port > 65535)
    throw new Error('HAI_SERV_MINIAPP_PORT must be an integer between 1 and 65535')
  return port
}

async function startServer(): Promise<void> {
  // 加载配置后按依赖顺序初始化；日志设置沿用 _core.yml。
  core.init({ configDir: './config' })
  await initializeModules()
  let mainServer: ServNodeServer | undefined
  let miniappServer: ServNodeServer | undefined
  try {
    // HOST/PORT 环境变量优先于 config/_serv.yml。
    const { server } = ServConfigSchema.parse(core.config.getOrThrow<ServConfigInput>('serv'))
    const miniappPort = parseMiniappPort(process.env.HAI_SERV_MINIAPP_PORT)
    const app = createServerApp()
    // 在监听前完成小程序 App 装配，配置非法时不留下半启动状态。
    const miniappApp = miniappPort === undefined ? undefined : createMiniappServerApp()
    const appName = core.config.getOrThrow<CoreConfig>('core').name
    mainServer = serv.listen(app, {
      host: server.host,
      port: server.port,
      onListening: info => core.logger.info('Application listening', { name: appName, host: server.host, port: info.port }),
      // 只有主通道拥有模块生命周期；小程序通道关闭时不得重复调用 closeModules。
      onClose: closeModules,
    })
    if (miniappApp && miniappPort !== undefined) {
      miniappServer = serv.listen(miniappApp, {
        host: server.host,
        port: miniappPort,
        onListening: info => core.logger.info('Miniapp channel listening', { name: appName, host: server.host, port: info.port, apiPrefix: MINIAPP_API_PREFIX, transport: 'disabled', httpsOnly: true }),
        // 只关闭本通道 HTTP 句柄；模块释放仍由主通道 onClose 统一负责。
        onClose: async () => undefined,
      })
    }
  }
  catch (error) {
    await miniappServer?.close().catch(() => undefined)
    await mainServer?.close().catch(() => undefined)
    await closeModules()
    throw error
  }
}

void startServer().catch((error: unknown) => {
  core.logger.fatal('Application startup failed', { error: error instanceof Error ? error.message : 'Unknown startup failure' })
  process.exitCode = 1
})
