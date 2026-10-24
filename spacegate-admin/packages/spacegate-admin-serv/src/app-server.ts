import type { CoreConfig } from '@h-ai/core'
import type { ServHttpApp } from '@h-ai/serv'
import type { CreateServerProceduresOptions } from './server/procedures/index.js'
import process from 'node:process'
import { core } from '@h-ai/core'
import { createReldbTransportKeyStore, crypto, parseTransportServerKeyRing } from '@h-ai/crypto'
import { iam } from '@h-ai/iam'
import { reldb } from '@h-ai/reldb'
import { serv, ServConfigSchema } from '@h-ai/serv'
import { storage } from '@h-ai/storage'
import * as contracts from 'spacegate-admin-contract'
import { createServerProcedures } from './server/procedures/index.js'

const STARTED_AT = new Date().toISOString()

/** 微信小程序原生通道的 API 前缀；与主通道 `/api/v1` 完全隔离。 */
export const MINIAPP_API_PREFIX = '/api/miniapp/v1'
/** 小程序通道只接受携带该客户端标识的请求，防止浏览器/未知客户端误用无加密通道。 */
export const MINIAPP_REQUEST_CLIENT = 'spacegate-admin-miniapp'

/** 两个通道复用同一套 contract、procedures 与 IAM 数据，只改变 HTTP 传输装配。 */
function createProceduresOptions(): CreateServerProceduresOptions {
  const coreConfig = core.config.getOrThrow<CoreConfig>('core')
  return {
    app: {
      name: coreConfig.name,
      version: coreConfig.version,
      env: coreConfig.env,
      defaultLocale: coreConfig.defaultLocale,
      supportedLocales: contracts.SUPPORTED_LOCALES,
      startedAt: STARTED_AT,
      frontends: [...contracts.ENABLED_FRONTENDS],
    },
  }
}

/**
 * 创建主管理通道 `/api/v1` 的 HTTP App。
 *
 * 该通道保持既有安全契约：业务 JSON 请求/响应强制 SM2 传输加密，
 * 浏览器 refresh token 走 HttpOnly Cookie，App/Desktop 走受信原生 body 通道。
 * @returns 可交给 `serv.listen` 的 HTTP App
 * @throws 传输密钥环缺失/非法或 serv/core 配置校验失败时抛出
 * @example
 * ```ts
 * const app = createServerApp()
 * const server = serv.listen(app, { host: '127.0.0.1', port: 3000 })
 * ```
 */
export function createServerApp(): ServHttpApp {
  const ring = parseTransportServerKeyRing(process.env.HAI_CRYPTO_TRANSPORT_SERVERKEYRING)
  if (!ring.success)
    throw new Error(ring.error.message)
  const coreConfig = core.config.getOrThrow<CoreConfig>('core')
  const servConfig = ServConfigSchema.parse(core.config.getOrThrow('serv'))
  const security = serv.createRuntimeSecurityPolicy({
    environment: coreConfig.env,
    corsOrigin: servConfig.cors.origin,
    nativeOrigins: servConfig.cors.nativeOrigins,
  })
  const publicPaths = [
    ...(servConfig.http.health ? [servConfig.http.health.path, servConfig.http.health.readyPath].filter((path): path is `/${string}` => Boolean(path)) : []),
    ...(servConfig.http.openapi && security.exposeApiDocs ? [servConfig.http.openapi.path] : []),
    ...(servConfig.http.docs && security.exposeApiDocs ? [servConfig.http.docs.path] : []),
  ]

  return serv.createApp({
    contract: contracts.spacegateAdminContract,
    procedures: createServerProcedures(createProceduresOptions()),
    iam,
    refreshCookie: {
      secure: security.secureRefreshCookie,
      nativeTokenTransport: { isRequest: request => request.headers.get('x-hai-token-transport') === 'body'
        && ['spacegate-admin-app', 'spacegate-admin-desktop'].includes(request.headers.get('x-request-client') ?? '')
        && security.isNativeOrigin(request.headers.get('origin') ?? '') },
    },
    // 管理业务默认强制双向加密；各节点复用受管服务端密钥环与共享 reldb 客户端 registry。
    transport: { ...(servConfig.transport || {}), excludePaths: publicPaths, crypto, serverKeyRing: ring.data, keyStore: createReldbTransportKeyStore({ reldb, ttlSeconds: 3600 }) },
    http: {
      ...servConfig.http,
      openapi: security.exposeApiDocs ? servConfig.http.openapi : false,
      docs: security.exposeApiDocs ? servConfig.http.docs : false,
    },
    middlewares: [{
      middleware: serv.cors({
        origin: security.allowOrigin,
        credentials: servConfig.cors.credentials,
        allowedHeaders: [...new Set([...servConfig.cors.allowedHeaders, 'X-Client-Id', 'X-Encrypted', 'X-Hai-Transport', 'X-Request-Client', 'X-HAI-Token-Transport'])],
        exposedHeaders: [...new Set([...servConfig.cors.exposedHeaders, 'X-Encrypted', 'X-Hai-Transport'])],
      }),
    }, {
      path: `${servConfig.http.apiPrefix}/assets/*`,
      middleware: serv.storageAssets({
        storage,
        pathPrefix: `${servConfig.http.apiPrefix}/assets/`,
        keyPattern: /^avatars\/[\w-]+\/[\w-]+\.(png|jpg|webp)$/,
        allowedContentTypes: ['image/png', 'image/jpeg', 'image/webp'],
        cacheControl: 'public, max-age=31536000, immutable',
        crossOriginResourcePolicy: 'cross-origin',
      }),
    }],
  })
}

/**
 * 创建微信小程序原生通道 `/api/miniapp/v1` 的 HTTP App。
 *
 * 微信 `wx.request` 无法实现主通道的 SM2 双向加密，因此该通道显式关闭
 * `transport`，只通过 HTTPS 反向代理保护传输；认证与授权仍复用同一 IAM
 * session 校验和每个 procedure 的 `admin` 角色守卫。refresh token 仅允许
 * 同时携带 `X-Request-Client: spacegate-admin-miniapp` 与
 * `X-HAI-Token-Transport: body` 的请求从 JSON body 读写，其他客户端必须走
 * 主通道。此通道默认不启动，仅在设置 `HAI_SERV_MINIAPP_PORT` 时由进程入口监听。
 *
 * 安全前置条件（部署方负责）：只绑定回环/内网地址并置于 HTTPS 反向代理之后，
 * 反向代理只转发受信来源；不要把该端口直接暴露到公网。无需该通道时不要设置
 * `HAI_SERV_MINIAPP_PORT`。
 * @returns 关闭传输加密、仅暴露小程序客户端的小程序 HTTP App
 * @throws serv/core 配置校验失败时抛出
 * @example
 * ```ts
 * // 仅当需要小程序通道时
 * const miniapp = createMiniappServerApp()
 * serv.listen(miniapp, { host: '127.0.0.1', port: 3001 })
 * ```
 */
export function createMiniappServerApp(): ServHttpApp {
  const servConfig = ServConfigSchema.parse(core.config.getOrThrow('serv'))
  return serv.createApp({
    contract: contracts.spacegateAdminContract,
    procedures: createServerProcedures(createProceduresOptions()),
    iam,
    // 小程序无法实现 SM2；native body 通道只对固定客户端标识开放，且必须 HTTPS 终止。
    refreshCookie: {
      secure: true,
      nativeTokenTransport: {
        isRequest: request => request.headers.get('x-request-client') === MINIAPP_REQUEST_CLIENT
          && request.headers.get('x-hai-token-transport') === 'body',
      },
    },
    // 显式不传 transport，等价于关闭加密；微信 wx.request 无法实现主通道的 SM2 通道。
    http: {
      ...servConfig.http,
      apiPrefix: MINIAPP_API_PREFIX,
      // 小程序通道不暴露 OpenAPI/docs，减少无加密面的探测入口。
      openapi: false,
      docs: false,
    },
    middlewares: [{
      // 健康检查保持公开；所有业务请求必须声明小程序客户端，避免其他客户端误接。
      path: `${MINIAPP_API_PREFIX}/*`,
      middleware: async (context, next) => {
        if (context.req.method === 'OPTIONS')
          return context.body(null, 204)
        if (context.req.header('x-request-client') !== MINIAPP_REQUEST_CLIENT) {
          return context.json({
            success: false,
            error: {
              code: 'MINIAPP_CLIENT_REQUIRED',
              message: `X-Request-Client must be ${MINIAPP_REQUEST_CLIENT}`,
              httpStatus: 403,
            },
          }, 403)
        }
        await next()
      },
    }],
  })
}
