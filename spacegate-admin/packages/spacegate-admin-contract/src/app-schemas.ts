import type { HaiResult } from '@h-ai/core'
import { z } from 'zod'

export const AppFrontendTargetSchema = z.enum(['web', 'app', 'miniapp', 'desktop'])

/** 服务信息响应数据。 */
export const AppInfoOutputDataSchema = z.object({
  name: z.string(),
  version: z.string(),
  env: z.string(),
  defaultLocale: z.string(),
  supportedLocales: z.array(z.string()),
  startedAt: z.string().datetime(),
  frontends: z.array(AppFrontendTargetSchema),
})

/** Echo 请求体。 */
export const AppEchoInputSchema = z.object({
  message: z.string().min(1).max(200),
})

/** Echo 响应数据。 */
export const AppEchoOutputDataSchema = z.object({
  message: z.string(),
  echoedAt: z.string().datetime(),
  requestId: z.string(),
})

export type AppFrontendTarget = z.infer<typeof AppFrontendTargetSchema>
export type AppInfoOutputData = z.infer<typeof AppInfoOutputDataSchema>
export type AppInfoOutput = HaiResult<AppInfoOutputData>
export type AppEchoInput = z.infer<typeof AppEchoInputSchema>
export type AppEchoOutputData = z.infer<typeof AppEchoOutputDataSchema>
export type AppEchoOutput = HaiResult<AppEchoOutputData>
