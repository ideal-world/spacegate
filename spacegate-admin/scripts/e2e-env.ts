import { createECDH } from 'node:crypto'
import { isAbsolute } from 'node:path'
import process from 'node:process'

/**
 * 仅给 Playwright 的服务子进程装配环境；不修改测试进程和 pnpm/Turbo 的环境。
 * @param profile - 本轮测试明确选择的模块配置和可信控制值。
 * @returns Playwright webServer.env；undefined 项由 Node spawn 从子进程环境中移除。
 * @throws 缺少绝对数据目录，或 API 指向非本轮 loopback HTTP 夹具时抛错。
 * @example
 * ```ts
 * // 测试内部上下文；数据目录由 run-e2e 为本轮分配。
 * const env = createE2eServerEnv({
 *   HAI_E2E_DATA_ROOT: process.cwd(),
 *   HAI_CACHE: JSON.stringify({ type: 'memory' }),
 * })
 * if (env.HAI_E2E !== '1') throw new Error('E2E profile was not applied')
 * ```
 */
export function createE2eServerEnv(profile: Record<string, string>): Record<string, string> {
  const dataRoot = profile.HAI_E2E_DATA_ROOT || process.env.HAI_E2E_DATA_ROOT
  if (!dataRoot || !isAbsolute(dataRoot))
    throw new Error('Run E2E through the managed entrypoint with its own absolute HAI_E2E_DATA_ROOT')
  if (profile.PUBLIC_API_BASE) {
    const api = new URL(profile.PUBLIC_API_BASE)
    if (api.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(api.hostname) || api.username || api.password)
      throw new Error('E2E API must use an unauthenticated loopback HTTP fixture')
  }
  const environment: Record<string, string | undefined> = Object.fromEntries(
    Object.keys(process.env).filter(key => /^(?:HAI_|PUBLIC_|VITE_)/i.test(key)).map(key => [key, undefined]),
  )
  for (const name of ['HAI_E2E_DATA_ROOT', 'HAI_AUTH_EVIDENCE_DIR', 'HAI_CORE_NAME', 'HAI_CORE_VERSION']) {
    if (process.env[name] !== undefined)
      environment[name] = process.env[name]
  }
  Object.assign(environment, { HAI_E2E: '1', NODE_ENV: 'test', HAI_E2E_DATA_ROOT: dataRoot }, profile)
  // Playwright 声明仅接受 string，但其 WebServer 合并 env 后直接交 Node spawn；
  // 已用安装版真实 WebServer 验证 undefined 会删除继承项，因此仅在第三方类型边界使用窄断言。
  return environment as Record<string, string>
}

/**
 * 每轮生成独立可信 SM2 ring；私钥只传服务子进程，不进入公开配置或报告。
 * @returns 符合 transport serverKeyRing 配置的 JSON 字符串。
 * @throws 当前 Node/OpenSSL 不支持 SM2 时抛错，不降级为不可信握手。
 * @example
 * ```ts
 * // 测试内部上下文；仅交给本轮服务子进程。
 * const env = createE2eServerEnv({
 *   HAI_E2E_DATA_ROOT: process.cwd(),
 *   HAI_CRYPTO_TRANSPORT_SERVERKEYRING: createE2eTransportRing(),
 * })
 * if (!env.HAI_CRYPTO_TRANSPORT_SERVERKEYRING) throw new Error('Trusted ring is required')
 * ```
 */
export function createE2eTransportRing(): string {
  const key = createECDH('SM2')
  key.generateKeys()
  return JSON.stringify({ activeKeyId: 'e2e-v1', keys: { 'e2e-v1': {
    publicKey: key.getPublicKey('hex'),
    privateKey: key.getPrivateKey('hex').padStart(64, '0'),
  } } })
}

/**
 * 只把既有的本轮 loopback 通知夹具控制地址转成明确 Reach 配置。
 * @returns 未启用通知夹具时为空对象；启用时包含 OTP、模板及协议配置。
 * @throws IAM_NOTIFICATION_URL 不是无凭据的 loopback HTTP 地址时抛错。
 * @example
 * ```ts
 * // 测试内部上下文；IAM_NOTIFICATION_URL 由样板门禁启动的通知夹具提供。
 * const env = createE2eServerEnv({
 *   ...createE2eNotificationEnv(),
 *   HAI_E2E_DATA_ROOT: process.cwd(),
 * })
 * if (env.HAI_E2E !== '1') throw new Error('Notification profile was not isolated')
 * ```
 */
export function createE2eNotificationEnv(): Record<string, string> {
  const address = process.env.IAM_NOTIFICATION_URL
  if (!address)
    return {}
  const url = new URL(address)
  if (url.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || url.username || url.password)
    throw new Error('E2E notification fixture must use an unauthenticated loopback HTTP address')
  return {
    HAI_IAM_LOGIN_OTP: 'true',
    HAI_IAM_PASSWORDRESET_ENABLED: 'true',
    HAI_REACH_PROVIDERS: JSON.stringify(['email', 'sms'].map(name => ({ name, type: 'api', url: new URL('/send', url).href }))),
    HAI_REACH_TEMPLATES: JSON.stringify([
      { name: 'otp_email', provider: 'email', body: 'Code: {code}' },
      { name: 'otp_sms', provider: 'sms', body: 'Code: {code}' },
      { name: 'password_reset', provider: 'email', body: 'Reset: {token}' },
      { name: 'password_reset_sms', provider: 'sms', body: 'Reset: {token}' },
    ]),
    HAI_IAM_AGREEMENTS: JSON.stringify({ showOnRegister: true, showOnLogin: true, userAgreementMarkdown: '# Configured registration agreement\n\n**Registration terms**\n\n- Keep your account credentials secure.', userAgreementUrl: new URL('/terms', url).href, privacyPolicyUrl: new URL('/privacy', url).href }),
  }
}
