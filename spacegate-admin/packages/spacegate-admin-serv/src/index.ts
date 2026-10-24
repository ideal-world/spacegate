import type { CoreConfig } from '@h-ai/core'
import type { ServConfigInput } from '@h-ai/serv'
import process from 'node:process'
import { core } from '@h-ai/core'
import { serv, ServConfigSchema } from '@h-ai/serv'
import { createServerApp } from './app-server.js'
import { closeModules, initializeModules } from './server/runtime.js'

async function startServer(): Promise<void> {
  // 加载配置后按依赖顺序初始化；日志设置沿用 _core.yml。
  core.init({ configDir: './config' })
  await initializeModules()
  try {
    // HOST/PORT 环境变量优先于 config/_serv.yml。
    const { server } = ServConfigSchema.parse(core.config.getOrThrow<ServConfigInput>('serv'))
    const app = createServerApp()
    serv.listen(app, {
      host: server.host,
      port: server.port,
      onListening: info => core.logger.info('Application listening', { name: core.config.getOrThrow<CoreConfig>('core').name, host: server.host, port: info.port }),
      onClose: closeModules,
    })
  }
  catch (error) {
    await closeModules()
    throw error
  }
}

void startServer().catch((error: unknown) => {
  core.logger.fatal('Application startup failed', { error: error instanceof Error ? error.message : 'Unknown startup failure' })
  process.exitCode = 1
})
