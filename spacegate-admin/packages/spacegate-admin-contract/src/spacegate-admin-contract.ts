import { apiContract } from '@h-ai/api-contract'
import { adminContract, authContract, profileContract } from './admin-contract.js'
import { appContract } from './app-contract.js'
import { spacegateContract } from './spacegate-contract.js'

// @hai-generator:contracts:imports

/** 当前工程启用的前端目标。 */
export const ENABLED_FRONTENDS = ['web', 'app', 'miniapp', 'desktop'] as const
export const SUPPORTED_LOCALES = ['zh-CN', 'en-US', 'ja-JP'] as const

/** 工程级 API contract，前端 typed client 与后端 service 共用。 */
export const spacegateAdminContract = apiContract.create({
  // @hai-generator:contracts:entries
  app: appContract,
  iam: { auth: authContract },
  admin: adminContract,
  profile: profileContract,
  spacegate: spacegateContract,
})

export type FullstackContract = typeof spacegateAdminContract
