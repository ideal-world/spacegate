import type { CoreConfig } from '@h-ai/core'
import type { ServHttpApp } from '@h-ai/serv'
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

/** 创建后端 HTTP App；与 @h-ai/serv 的 createApp 返回语义保持一致。 */
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
    procedures: createServerProcedures({
      app: {
        name: coreConfig.name,
        version: coreConfig.version,
        env: coreConfig.env,
        defaultLocale: coreConfig.defaultLocale,
        supportedLocales: contracts.SUPPORTED_LOCALES,
        startedAt: STARTED_AT,
        frontends: [...contracts.ENABLED_FRONTENDS],
      },
    }),
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
