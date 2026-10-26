/**
 * 确定性 SpaceGate 上游夹具：模拟 Rust admin-server 与 AI Gateway 的管理接口。
 *
 * 仅用于 Playwright：服务进程通过 HTTP 访问本夹具，浏览器不直接连接。
 * 版本语义与 `binary/admin-server/src/mw/version_control.rs` 保持一致：
 * - GET 响应携带 `x-server-version`；
 * - 版本化写操作要求 `x-client-version` 等于当前版本，否则 409 并返回服务端版本；
 * - 写成功后版本加一；读成功后不改变版本；
 * - `__bump` 只推进版本，便于制造冲突而不刷新界面。
 *
 * 不记录请求体、头部或凭据；启动日志只输出监听地址。
 * @example
 * ```bash
 * MOCK_UPSTREAM_PORT=3998 node e2e/mock-spacegate-upstream.mjs
 * curl -s http://127.0.0.1:3998/config/names
 * ```
 */
import { Buffer } from 'node:buffer'
import { createServer } from 'node:http'
import process from 'node:process'

const host = process.env.MOCK_UPSTREAM_HOST || '127.0.0.1'
const port = Number(process.env.MOCK_UPSTREAM_PORT || '3998')
const initialVersion = 1

/** 创建全新的上游状态；每个夹具进程与 `__reset` 共用同一份定义。 */
function createState() {
  return {
    version: initialVersion,
    // 版本化写请求次数（含 409）；用于断言冲突后没有自动重试。
    versionedWriteAttempts: 0,
    gateways: {
      'gateway-a': {
        name: 'gateway-a',
        listeners: [{
          name: 'http',
          ip: null,
          port: 8080,
          hostname: null,
          protocol: { type: 'http' },
        }],
        parameters: {
          redis_url: 'redis://127.0.0.1:6379',
          redis_pool_max_size: 16,
          redis_pool_wait_timeout_ms: 1000,
          redis_pool_create_timeout_ms: 1000,
          redis_pool_recycle_timeout_ms: 10000,
          log_level: 'info',
          lang: null,
          enable_x_request_id: true,
          ignore_tls_verification: false,
          observability: { log: { level: 'info' } },
        },
        plugins: [],
      },
    },
    routes: {
      'gateway-a': {
        'route-a': {
          route_name: 'route-a',
          priority: 0,
          hostnames: null,
          plugins: [],
          rules: [{
            matches: [{
              path: { kind: 'Prefix', value: '/api', replace: null },
              header: null,
              query: null,
              method: null,
            }],
            backends: [{
              host: { kind: 'Host', host: '127.0.0.1' },
              port: 9000,
              protocol: null,
              weight: 1,
              timeout_ms: null,
              timeout_mode: 'disabled',
              downgrade_http2: false,
              plugins: [],
            }],
            timeout_ms: null,
            balance_policy: null,
            plugins: [],
          }],
        },
      },
    },
    plugins: [{
      code: 'basic',
      kind: 'named',
      name: 'basic-a',
      display_name: 'Basic auth',
      spec: { users: [] },
    }],
    pluginCatalog: ['basic'],
    pluginAttrs: [{
      code: 'basic',
      mono: false,
      meta: { title: 'Basic auth', description: 'Deterministic fixture plugin', version: '1.0.0' },
    }],
    instances: ['instance-a'],
    health: { 'instance-a': true },
    aiRules: [{
      tenant: 'tenant-a',
      model: null,
      path: null,
      policy: 'queue',
      rps: 20,
      burst: 40,
      ttl_seconds: 60,
    }],
  }
}

let state = createState()

/** 版本化写操作：只读 POST 与夹具控制端点不参与版本校验。 */
function requiresVersion(method, pathname) {
  if (method !== 'POST' && method !== 'PUT' && method !== 'DELETE')
    return false
  if (pathname.startsWith('/__'))
    return false
  return !(
    pathname.endsWith('/wasm/schema/preview')
    || pathname === '/plugin/refresh'
    || pathname.endsWith('/plugin/refresh')
  )
}

function json(res, status, body, headers = {}) {
  const payload = body === undefined ? '' : JSON.stringify(body)
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
    ...headers,
  })
  res.end(payload)
}

/** GET 响应统一附带当前版本，模拟 Rust 版本中间件。 */
function versionedJson(res, body) {
  json(res, 200, body, { 'x-server-version': String(state.version) })
}

function text(res, status, body, contentType) {
  res.writeHead(status, {
    'content-type': contentType,
    'content-length': Buffer.byteLength(body),
  })
  res.end(body)
}

/** 读取请求体；上限只防止夹具被异常输入拖垮。 */
async function readJson(req) {
  const chunks = []
  let size = 0
  for await (const chunk of req) {
    size += chunk.length
    if (size > 2 * 1024 * 1024)
      throw new Error('request body too large')
    chunks.push(chunk)
  }
  if (chunks.length === 0)
    return undefined
  const raw = Buffer.concat(chunks).toString('utf8')
  try {
    return JSON.parse(raw)
  }
  catch {
    return undefined
  }
}

function decodeSegment(value) {
  return decodeURIComponent(value)
}

/** 插件实例身份匹配；查询参数与 Rust 的 PluginInstanceId 对齐。 */
function pluginMatches(plugin, query) {
  if (plugin.code !== query.get('code'))
    return false
  if (plugin.kind !== query.get('kind'))
    return false
  if (plugin.kind === 'named')
    return plugin.name === query.get('name')
  if (plugin.kind === 'anon')
    return plugin.uid === query.get('uid')
  return true
}

/** 规范化插件查询名：完整 query 对象，便于复用匹配逻辑。 */
function queryOf(url) {
  return url.searchParams
}

function upsertPlugin(query, spec) {
  const code = query.get('code') || ''
  const kind = query.get('kind') || 'named'
  const displayName = query.get('display_name') || undefined
  const id = kind === 'anon'
    ? { code, kind, uid: query.get('uid') || 'anon-1' }
    : { code, kind: 'named', name: query.get('name') || 'plugin-1' }
  const current = state.plugins.find(item => item.code === id.code
    && item.kind === id.kind
    && (id.kind === 'named' ? item.name === id.name : item.uid === id.uid))
  const next = { ...id, ...(displayName ? { display_name: displayName } : {}), spec: spec ?? {} }
  if (current)
    Object.assign(current, next)
  else
    state.plugins.push(next)
  return next
}

function aiRuleKey(rule) {
  return `${rule.tenant ?? ''}\u0000${rule.model ?? ''}\u0000${rule.path ?? ''}\u0000${rule.policy ?? ''}`
}

function filterAiRules(url) {
  const filters = [...url.searchParams.entries()].filter(([, value]) => value !== '')
  return state.aiRules.filter(rule => filters.every(([key, value]) => String(rule[key] ?? '') === value))
}

/** 分发请求；返回 false 表示未匹配任何路由。 */
async function dispatch(req, res, url) {
  const { method } = req
  const pathname = url.pathname

  if (method === 'GET' && (pathname === '/__state' || pathname === '/__health')) {
    json(res, 200, state)
    return true
  }
  if (method === 'POST' && pathname === '/__reset') {
    state = createState()
    json(res, 200, state)
    return true
  }
  if (method === 'POST' && pathname === '/__bump') {
    state.version += 1
    json(res, 200, { version: state.version })
    return true
  }

  if (method === 'GET' && pathname === '/config/names') {
    versionedJson(res, Object.keys(state.gateways))
    return true
  }
  if (method === 'GET' && pathname === '/config/plugin-all') {
    versionedJson(res, state.plugins)
    return true
  }
  if (method === 'GET' && pathname === '/plugin/list') {
    versionedJson(res, state.pluginCatalog)
    return true
  }
  if (method === 'GET' && pathname === '/plugin/attr-all') {
    versionedJson(res, state.pluginAttrs)
    return true
  }
  if (method === 'GET' && pathname === '/discovery/backends') {
    versionedJson(res, [])
    return true
  }
  if (method === 'GET' && (pathname === '/discovery/instance/list' || pathname === '/instance/list')) {
    versionedJson(res, state.instances)
    return true
  }
  if (method === 'GET' && (pathname === '/discovery/instance/health' || pathname === '/instance/health')) {
    versionedJson(res, state.health)
    return true
  }
  if (method === 'GET' && pathname.startsWith('/discovery/instance/reload')) {
    json(res, 405, { error: 'reload is a POST endpoint' }, { 'x-server-version': String(state.version) })
    return true
  }

  const pluginSchema = pathname.match(/^\/plugin\/schema\/([^/]+)$/)
  if (method === 'GET' && pluginSchema) {
    versionedJson(res, { type: 'object', title: decodeSegment(pluginSchema[1]), properties: {} })
    return true
  }
  const pluginAttr = pathname.match(/^\/plugin\/attr\/([^/]+)$/)
  if (method === 'GET' && pluginAttr) {
    versionedJson(res, state.pluginAttrs.find(item => item.code === decodeSegment(pluginAttr[1])) ?? null)
    return true
  }
  if (method === 'GET' && pathname === '/plugin/wasm/schema') {
    versionedJson(res, { type: 'object', properties: {} })
    return true
  }
  if (method === 'POST' && pathname === '/plugin/wasm/schema/preview') {
    await readJson(req)
    json(res, 200, { type: 'object', properties: { preview: { type: 'string' } } })
    return true
  }
  if (method === 'POST' && pathname === '/plugin/refresh') {
    json(res, 200, state.pluginAttrs)
    return true
  }
  if (method === 'GET' && pathname === '/config/plugin') {
    const query = queryOf(url)
    versionedJson(res, state.plugins.find(item => pluginMatches(item, query)) ?? null)
    return true
  }
  const pluginsByCode = pathname.match(/^\/config\/plugins\/([^/]+)$/)
  if (method === 'GET' && pluginsByCode) {
    const code = decodeSegment(pluginsByCode[1])
    versionedJson(res, state.plugins.filter(item => item.code === code))
    return true
  }

  const gatewayPath = pathname.match(/^\/config\/item\/([^/]+)\/gateway$/)
  if (gatewayPath) {
    const name = decodeSegment(gatewayPath[1])
    if (method === 'GET') {
      versionedJson(res, state.gateways[name] ?? null)
      return true
    }
    if (method === 'POST' || method === 'PUT') {
      const body = await readJson(req)
      state.gateways[name] = { ...(typeof body === 'object' && body !== null ? body : {}), name }
      state.routes[name] ??= {}
      return 'mutated'
    }
    if (method === 'DELETE') {
      delete state.gateways[name]
      delete state.routes[name]
      return 'mutated'
    }
  }

  const routeAll = pathname.match(/^\/config\/item\/([^/]+)\/route\/all$/)
  if (method === 'GET' && routeAll) {
    versionedJson(res, state.routes[decodeSegment(routeAll[1])] ?? {})
    return true
  }
  const routeNames = pathname.match(/^\/config\/item\/([^/]+)\/route\/names$/)
  if (method === 'GET' && routeNames) {
    versionedJson(res, Object.keys(state.routes[decodeSegment(routeNames[1])] ?? {}))
    return true
  }
  const routeRename = pathname.match(/^\/config\/item\/([^/]+)\/route\/item\/([^/]+)\/rename$/)
  if (method === 'POST' && routeRename) {
    const gateway = decodeSegment(routeRename[1])
    const route = decodeSegment(routeRename[2])
    const body = await readJson(req)
    const newName = typeof body?.route_name === 'string' ? body.route_name : ''
    if (!newName || !state.routes[gateway]?.[route])
      return 'missing'
    delete state.routes[gateway][route]
    state.routes[gateway][newName] = { ...body, route_name: newName }
    return 'mutated'
  }
  const routeItem = pathname.match(/^\/config\/item\/([^/]+)\/route\/item\/([^/]+)$/)
  if (routeItem) {
    const gateway = decodeSegment(routeItem[1])
    const route = decodeSegment(routeItem[2])
    if (method === 'GET') {
      const existing = state.routes[gateway]?.[route]
      versionedJson(res, existing ?? null)
      return true
    }
    if (method === 'POST' || method === 'PUT') {
      const body = await readJson(req)
      state.routes[gateway] ??= {}
      state.routes[gateway][route] = { ...(typeof body === 'object' && body !== null ? body : {}), route_name: route }
      return 'mutated'
    }
    if (method === 'DELETE') {
      if (!state.routes[gateway]?.[route])
        return 'missing'
      delete state.routes[gateway][route]
      return 'mutated'
    }
  }

  const configItem = pathname.match(/^\/config\/item\/([^/]+)$/)
  if (method === 'GET' && configItem) {
    versionedJson(res, state.gateways[decodeSegment(configItem[1])] ?? null)
    return true
  }

  if (method === 'PUT' && pathname === '/config') {
    await readJson(req)
    return 'mutated'
  }

  if (method === 'POST' && pathname === '/config/plugin') {
    const spec = await readJson(req)
    upsertPlugin(queryOf(url), spec)
    return 'mutated'
  }
  if (method === 'PUT' && pathname === '/config/plugin') {
    const spec = await readJson(req)
    upsertPlugin(queryOf(url), spec)
    return 'mutated'
  }
  if (method === 'DELETE' && pathname === '/config/plugin') {
    const query = queryOf(url)
    const index = state.plugins.findIndex(item => pluginMatches(item, query))
    if (index >= 0)
      state.plugins.splice(index, 1)
    return 'mutated'
  }

  if (method === 'POST' && (pathname === '/discovery/instance/reload/gateway'
    || pathname === '/discovery/instance/reload/global'
    || pathname === '/discovery/instance/reload/route')) {
    return 'mutated'
  }

  const aiPlugin = pathname.match(/^\/v1\/admin\/plugins\/([^/]+)\/(schema|readme)$/)
  if (method === 'GET' && aiPlugin) {
    if (aiPlugin[2] === 'schema') {
      json(res, 200, {
        type: 'object',
        title: 'AI gateway queue',
        properties: {
          rps: { type: 'number' },
          burst: { type: 'number' },
          policy: { type: 'string', enum: ['abandon', 'queue', 'wait'] },
        },
      })
      return true
    }
    text(res, 200, '# AI gateway queue\n\nDeterministic fixture readme.\n', 'text/markdown; charset=utf-8')
    return true
  }
  if (method === 'GET' && pathname === '/v1/admin/tenant-rate-limits/resolve') {
    const tenant = url.searchParams.get('tenant') || ''
    json(res, 200, {
      tenant,
      model: url.searchParams.get('model'),
      path: url.searchParams.get('path'),
      policy: url.searchParams.get('policy') || 'queue',
      rule: state.aiRules.find(item => item.tenant === tenant) ?? null,
    })
    return true
  }
  if (pathname === '/v1/admin/tenant-rate-limits') {
    if (method === 'GET') {
      json(res, 200, filterAiRules(url))
      return true
    }
    const rule = await readJson(req)
    if (method === 'PUT') {
      const next = typeof rule === 'object' && rule !== null ? rule : {}
      const index = state.aiRules.findIndex(item => aiRuleKey(item) === aiRuleKey(next))
      if (index >= 0)
        state.aiRules[index] = next
      else
        state.aiRules.push(next)
      json(res, 200, next)
      return true
    }
    if (method === 'DELETE') {
      const key = aiRuleKey(typeof rule === 'object' && rule !== null ? rule : {})
      const index = state.aiRules.findIndex(item => aiRuleKey(item) === key)
      if (index >= 0)
        state.aiRules.splice(index, 1)
      json(res, 200, { deleted: index >= 0 ? 1 : 0 })
      return true
    }
  }

  return false
}

const server = createServer((req, res) => {
  void (async () => {
    let url
    try {
      url = new URL(req.url ?? '/', `http://${host}:${port}`)
    }
    catch {
      json(res, 400, { error: 'invalid url' })
      return
    }

    if (requiresVersion(req.method ?? 'GET', url.pathname)) {
      state.versionedWriteAttempts += 1
      const clientVersion = Number(req.headers['x-client-version'])
      if (!Number.isInteger(clientVersion) || clientVersion !== state.version) {
        // 与 Rust 一致：冲突响应只暴露服务端版本，不执行任何写操作。
        json(res, 409, undefined, { 'x-server-version': String(state.version) })
        return
      }
    }

    let outcome
    try {
      outcome = await dispatch(req, res, url)
    }
    catch {
      json(res, 400, { error: 'invalid request' })
      return
    }
    if (outcome === false) {
      json(res, 404, { error: 'not found' })
      return
    }
    if (outcome === 'missing') {
      json(res, 404, { error: 'not found' })
      return
    }
    if (outcome === 'mutated') {
      // 版本化写成功后才推进版本；响应体保持空对象，客户端会重新读取 /config/names。
      state.version += 1
      json(res, 200, {}, { 'x-server-version': String(state.version) })
    }
  })()
})

server.listen(port, host, () => {
  process.stdout.write(`mock-spacegate-upstream listening on http://${host}:${port}\n`)
})

function shutdown() {
  server.close(() => process.exit(0))
}
process.once('SIGINT', shutdown)
process.once('SIGTERM', shutdown)
