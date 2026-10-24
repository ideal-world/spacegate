import type { CreateAppProceduresOptions } from './app-procedures.js'
import { createSpacegateUpstreamClient } from '../upstream/spacegate-upstream-client.js'
import { createAdminProcedures, createAuthProcedures, createProfileProcedures } from './admin-procedures.js'
import { createAppProcedures } from './app-procedures.js'
import { createSpacegateProcedures } from './spacegate-procedures.js'

// @hai-generator:procedures:imports

export interface CreateServerProceduresOptions {
  readonly app: CreateAppProceduresOptions
}

/** 工程级 procedures 聚合入口；后续增加领域时在这里追加即可。 */
export function createServerProcedures(options: CreateServerProceduresOptions) {
  // 上游客户端在 App 装配时创建一次，复用进程内 JWT 与最近版本快照。
  const spacegateClient = createSpacegateUpstreamClient()
  return {
    // @hai-generator:procedures:entries
    app: createAppProcedures(options.app),
    iam: { auth: createAuthProcedures() },
    admin: createAdminProcedures(),
    profile: createProfileProcedures(),
    spacegate: createSpacegateProcedures(spacegateClient),
  }
}
