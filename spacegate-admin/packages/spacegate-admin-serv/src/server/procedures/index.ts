import type { CreateAppProceduresOptions } from './app-procedures.js'
import { createAdminProcedures, createAuthProcedures, createProfileProcedures } from './admin-procedures.js'
import { createAppProcedures } from './app-procedures.js'

// @hai-generator:procedures:imports

export interface CreateServerProceduresOptions {
  readonly app: CreateAppProceduresOptions
}

/** 工程级 procedures 聚合入口；后续增加领域时在这里追加即可。 */
export function createServerProcedures(options: CreateServerProceduresOptions) {
  return {
    // @hai-generator:procedures:entries
    app: createAppProcedures(options.app),
    iam: { auth: createAuthProcedures() },
    admin: createAdminProcedures(),
    profile: createProfileProcedures(),
  }
}
