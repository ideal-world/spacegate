import type { ServContext } from '@h-ai/serv'
import type { AppEchoOutputData, AppFrontendTarget, AppInfoOutputData } from 'spacegate-admin-contract'
import { ok } from '@h-ai/core'
import { serv } from '@h-ai/serv'
import { appContract } from 'spacegate-admin-contract'

export interface CreateAppProceduresOptions {
  readonly name: string
  readonly version: string
  readonly env: string
  readonly defaultLocale: string
  readonly supportedLocales: readonly string[]
  readonly startedAt: string
  readonly frontends: readonly AppFrontendTarget[]
}

/** app 领域 procedures：把共享 contract 绑定到真实业务处理函数。 */
export function createAppProcedures(options: CreateAppProceduresOptions) {
  return serv
    .implement(appContract)
    .context<ServContext>()
    .route('info', () => ok(createAppInfo(options)))
    .route('echo', ({ input, context }) => {
      const data: AppEchoOutputData = {
        message: input.message,
        echoedAt: new Date().toISOString(),
        requestId: context.requestId,
      }
      return ok(data)
    })
    .build()
}

function createAppInfo(options: CreateAppProceduresOptions): AppInfoOutputData {
  return {
    name: options.name,
    version: options.version,
    env: options.env,
    defaultLocale: options.defaultLocale,
    supportedLocales: [...options.supportedLocales],
    startedAt: options.startedAt,
    frontends: [...options.frontends],
  }
}
