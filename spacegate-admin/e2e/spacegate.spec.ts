import type { APIRequestContext, Page } from '@playwright/test'
import { randomUUID } from 'node:crypto'
import process from 'node:process'
import { expect, test } from '@playwright/test'

/**
 * SpaceGate 管理端浏览器验收：Web/App/Desktop 共用共享页面、contract 与 store。
 *
 * 服务进程连接 `e2e/mock-spacegate-upstream.mjs`，因此断言直接读取夹具状态，
 * 不写真实 Rust 配置文件，也不依赖外部 Kubernetes / AI Gateway。
 */
const mockBaseUrl = process.env.MOCK_UPSTREAM_BASE_URL
  || `http://127.0.0.1:${process.env.MOCK_UPSTREAM_PORT || '3998'}`
const adminPassword = process.env.E2E_ADMIN_PASSWORD || 'Scaffold-test-Admin123!'
const frontends = {
  web: process.env.HAI_E2E_WEB_URL || 'http://127.0.0.1:4173',
  app: process.env.HAI_E2E_APP_URL || 'http://127.0.0.1:5175',
  desktop: process.env.HAI_E2E_DESKTOP_URL || 'http://127.0.0.1:5176',
}

/** 夹具状态的可断言子集；未知字段保持宽松，避免测试绑定全部上游结构。 */
interface MockState {
  version: number
  versionedWriteAttempts: number
  gateways: Record<string, Record<string, unknown>>
  routes: Record<string, Record<string, Record<string, unknown>>>
  plugins: Array<Record<string, unknown>>
  instances: string[]
  health: Record<string, boolean>
  aiRules: Array<Record<string, unknown>>
}

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {}
}

function gatewayOf(state: MockState, name: string): Record<string, unknown> {
  return record(state.gateways[name])
}

function listenerPortOf(state: MockState, name: string): unknown {
  const listeners = gatewayOf(state, name).listeners
  return Array.isArray(listeners) ? record(listeners[0]).port : undefined
}

function redisUrlOf(state: MockState, name: string): unknown {
  return record(gatewayOf(state, name).parameters).redis_url
}

function routeOf(state: MockState, gateway: string, route: string): Record<string, unknown> {
  return record(record(state.routes[gateway])[route])
}

function routePriorityOf(state: MockState, gateway: string, route: string): unknown {
  return routeOf(state, gateway, route).priority
}

function routePathOf(state: MockState, gateway: string, route: string): unknown {
  const rules = routeOf(state, gateway, route).rules
  const firstRule = Array.isArray(rules) ? record(rules[0]) : {}
  const matches = firstRule.matches
  const firstMatch = Array.isArray(matches) ? record(matches[0]) : {}
  return record(firstMatch.path).value
}

async function readState(request: APIRequestContext): Promise<MockState> {
  const response = await request.get(`${mockBaseUrl}/__state`)
  expect(response.ok()).toBe(true)
  return await response.json() as MockState
}

/** 解析 JSON 编辑器内容，修改后重新序列化；非对象直接失败，避免静默覆盖。 */
function editJson(text: string, mutate: (draft: Record<string, unknown>) => void): string {
  const parsed: unknown = JSON.parse(text)
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed))
    throw new Error('JSON editor does not contain an object')
  const draft = parsed as Record<string, unknown>
  mutate(draft)
  return JSON.stringify(draft, null, 2)
}

async function submitLogin(page: Page, username: string, password: string) {
  await expect(page.getByTestId('login-form')).toBeVisible()
  await page.locator('#login-username').fill(username)
  await page.getByTestId('login-form').locator('input[type="password"]').fill(password)
  await page.getByTestId('login-form').getByRole('button', { name: /登录|Sign in|Login|Log in/i }).click()
}

/**
 * 登录并关闭首次引导提示；引导异步恢复，先等待它出现再决定是否关闭。
 * @param page - 目标前端页面
 * @param url - Web/App/Desktop 预览地址
 */
async function login(page: Page, url: string) {
  await page.goto(url)
  await submitLogin(page, 'admin', adminPassword)
  await expect(page.getByTestId('logout')).toBeVisible()
  await expect(page.getByTestId('overview-page')).toBeVisible()
  const later = page.getByTestId('guide-later')
  await later.waitFor({ state: 'visible', timeout: 2_000 }).catch(() => undefined)
  if (await later.isVisible())
    await later.click()
}

/** 打开唯一网关的编辑器；桌面表格与移动卡片都通过可见按钮点击。 */
async function openGatewayEditor(page: Page) {
  const edit = page.getByTestId('gateways-page').locator('button:visible').filter({ hasText: /编辑|Edit/ }).first()
  await edit.click()
  await expect(page.getByTestId('gateway-editor')).toBeVisible()
  // 编辑器打开后异步读取完整配置；等到真实网关名出现再填写，避免覆盖默认草稿。
  await expect(page.getByTestId('gateway-json').locator('textarea')).toHaveValue(/"name":\s*"gateway-a"/)
}

/** 打开唯一路由的编辑器。 */
async function openRouteEditor(page: Page) {
  const edit = page.getByTestId('routes-page').locator('button:visible').filter({ hasText: /编辑|Edit/ }).first()
  await edit.click()
  await expect(page.getByTestId('route-editor')).toBeVisible()
  await expect(page.getByTestId('route-json').locator('textarea')).toHaveValue(/"route_name":\s*"route-a"/)
}

async function saveGateway(page: Page) {
  await page.getByTestId('gateway-save').click()
  await expect(page.getByTestId('gateway-editor')).not.toBeVisible()
}

async function saveRoute(page: Page) {
  await page.getByTestId('route-save').click()
  await expect(page.getByTestId('route-editor')).not.toBeVisible()
}

test.beforeEach(async ({ request }) => {
  // 每个用例从相同版本与配置开始，避免 Web/App/Desktop 串行执行时互相污染。
  const response = await request.post(`${mockBaseUrl}/__reset`)
  expect(response.ok()).toBe(true)
})

test('spacegate web: overview counts and gateway/route writes reach the upstream fixture', async ({ page, request }) => {
  test.setTimeout(60_000)
  await login(page, frontends.web)

  await expect(page.getByTestId('overview-gateways')).toHaveText('1')
  await expect(page.getByTestId('overview-routes')).toHaveText('1')
  await expect(page.getByTestId('overview-plugins')).toHaveText('1')
  await expect(page.getByTestId('overview-health')).toHaveText('1/1')

  await page.getByTestId('nav-gateways').click()
  await expect(page.getByTestId('gateways-page')).toBeVisible()
  await openGatewayEditor(page)

  const gatewayJson = page.getByTestId('gateway-json').locator('textarea')
  await gatewayJson.fill(editJson(await gatewayJson.inputValue(), (draft) => {
    const listeners = Array.isArray(draft.listeners) ? draft.listeners : []
    const listener = record(listeners[0])
    listener.port = 18081
    listeners[0] = listener
    draft.listeners = listeners
    const parameters = record(draft.parameters)
    parameters.redis_url = 'redis://127.0.0.1:6391'
    draft.parameters = parameters
  }))
  const beforeGatewaySave = await readState(request)
  await saveGateway(page)
  await expect(page.getByTestId('notice')).toBeVisible()

  const afterGatewaySave = await readState(request)
  expect(afterGatewaySave.version).toBe(beforeGatewaySave.version + 1)
  expect(listenerPortOf(afterGatewaySave, 'gateway-a')).toBe(18081)
  expect(redisUrlOf(afterGatewaySave, 'gateway-a')).toBe('redis://127.0.0.1:6391')

  await page.getByTestId('nav-routes').click()
  await expect(page.getByTestId('routes-page')).toBeVisible()
  await openRouteEditor(page)

  const routeJson = page.getByTestId('route-json').locator('textarea')
  await routeJson.fill(editJson(await routeJson.inputValue(), (draft) => {
    draft.priority = 7
    const rules = Array.isArray(draft.rules) ? draft.rules : []
    const rule = record(rules[0])
    const matches = Array.isArray(rule.matches) ? rule.matches : []
    const match = record(matches[0])
    match.path = { ...record(match.path), value: '/api/v2' }
    matches[0] = match
    rule.matches = matches
    rules[0] = rule
    draft.rules = rules
  }))
  const beforeRouteSave = await readState(request)
  await saveRoute(page)

  const afterRouteSave = await readState(request)
  expect(afterRouteSave.version).toBe(beforeRouteSave.version + 1)
  expect(routePriorityOf(afterRouteSave, 'gateway-a', 'route-a')).toBe(7)
  expect(routePathOf(afterRouteSave, 'gateway-a', 'route-a')).toBe('/api/v2')
})

test('spacegate web: version conflict requires explicit reload and never retries automatically', async ({ page, request }) => {
  test.setTimeout(60_000)
  await login(page, frontends.web)
  await page.getByTestId('nav-gateways').click()
  await expect(page.getByTestId('gateways-page')).toBeVisible()

  // 先完成一次正常写入，获得编辑器的最新读版本。
  await openGatewayEditor(page)
  const gatewayJson = page.getByTestId('gateway-json').locator('textarea')
  await gatewayJson.fill(editJson(await gatewayJson.inputValue(), (draft) => {
    const listeners = Array.isArray(draft.listeners) ? draft.listeners : []
    const listener = record(listeners[0])
    listener.port = 18091
    listeners[0] = listener
    draft.listeners = listeners
  }))
  await saveGateway(page)
  const saved = await readState(request)
  expect(listenerPortOf(saved, 'gateway-a')).toBe(18091)

  // 打开编辑器建立读版本快照，再推进服务端版本，模拟其他管理员已提交配置。
  await openGatewayEditor(page)
  await gatewayJson.fill(editJson(await gatewayJson.inputValue(), (draft) => {
    const listeners = Array.isArray(draft.listeners) ? draft.listeners : []
    const listener = record(listeners[0])
    listener.port = 18092
    listeners[0] = listener
    draft.listeners = listeners
  }))
  const bump = await request.post(`${mockBaseUrl}/__bump`)
  expect(bump.ok()).toBe(true)
  const bumped = await readState(request)
  expect(bumped.version).toBe(saved.version + 1)

  // 过期版本保存：显示冲突横幅，草稿保留，上游不被覆盖。
  const attemptsBeforeConflict = bumped.versionedWriteAttempts
  await page.getByTestId('gateway-save').click()
  const editor = page.getByTestId('gateway-editor')
  await expect(editor.getByTestId('version-conflict')).toBeVisible()
  await expect(editor).toBeVisible()

  const conflicted = await readState(request)
  expect(conflicted.version).toBe(bumped.version)
  expect(conflicted.versionedWriteAttempts).toBe(attemptsBeforeConflict + 1)
  expect(listenerPortOf(conflicted, 'gateway-a')).toBe(18091)

  // 给潜在的自动重试留出时间：版本、写次数和配置都必须保持不变。
  await page.waitForTimeout(600)
  const stillConflicted = await readState(request)
  expect(stillConflicted.version).toBe(bumped.version)
  expect(stillConflicted.versionedWriteAttempts).toBe(attemptsBeforeConflict + 1)
  expect(listenerPortOf(stillConflicted, 'gateway-a')).toBe(18091)

  // 显式刷新关闭过期编辑器并重新读取版本。
  await editor.getByTestId('version-conflict-reload').click()
  await expect(page.getByTestId('gateway-editor')).not.toBeVisible()

  await openGatewayEditor(page)
  await gatewayJson.fill(editJson(await gatewayJson.inputValue(), (draft) => {
    const listeners = Array.isArray(draft.listeners) ? draft.listeners : []
    const listener = record(listeners[0])
    listener.port = 18093
    listeners[0] = listener
    draft.listeners = listeners
  }))
  await saveGateway(page)

  const recovered = await readState(request)
  expect(recovered.version).toBe(bumped.version + 1)
  expect(listenerPortOf(recovered, 'gateway-a')).toBe(18093)
})

for (const frontend of [
  { target: 'app', url: frontends.app, viewport: { width: 390, height: 844 }, port: 18101 },
  { target: 'desktop', url: frontends.desktop, viewport: { width: 1440, height: 960 }, port: 18111 },
] as const) {
  test(`spacegate ${frontend.target}: preview logs in, shows overview and writes a gateway upstream`, async ({ browser, request }) => {
    test.setTimeout(60_000)
    const page = await browser.newPage({ viewport: frontend.viewport })
    const pageErrors: string[] = []
    page.on('pageerror', error => pageErrors.push(error.message))
    try {
      await page.goto(frontend.url)
      await submitLogin(page, 'admin', adminPassword)
      await expect(page.getByTestId('logout')).toBeVisible()
      await expect(page.getByTestId('overview-page')).toBeVisible()
      await expect(page.getByTestId('overview-gateways')).toHaveText('1')
      await expect(page.getByTestId('overview-health')).toHaveText('1/1')
      const later = page.getByTestId('guide-later')
      await later.waitFor({ state: 'visible', timeout: 2_000 }).catch(() => undefined)
      if (await later.isVisible())
        await later.click()

      if (frontend.target === 'app') {
        await page.getByTestId('mobile-menu').click()
      }
      await page.getByTestId('nav-gateways').click()
      await expect(page.getByTestId('gateways-page')).toBeVisible()

      await openGatewayEditor(page)
      const gatewayJson = page.getByTestId('gateway-json').locator('textarea')
      await gatewayJson.fill(editJson(await gatewayJson.inputValue(), (draft) => {
        const listeners = Array.isArray(draft.listeners) ? draft.listeners : []
        const listener = record(listeners[0])
        listener.port = frontend.port
        listeners[0] = listener
        draft.listeners = listeners
      }))
      await saveGateway(page)

      const state = await readState(request)
      expect(listenerPortOf(state, 'gateway-a')).toBe(frontend.port)
      expect(pageErrors).toEqual([])
    }
    finally {
      await page.close()
    }
  })
}

test('spacegate non-admin: only personal settings are visible and management hash routes fall back', async ({ page, browser }) => {
  test.setTimeout(90_000)
  const username = `spacegate-member-${randomUUID().replaceAll('-', '').slice(0, 10)}`
  const password = 'Member-test-Pass123!'

  await login(page, frontends.web)
  await page.getByTestId('nav-users').click()
  await expect(page.getByTestId('users-page')).toBeVisible()
  await page.getByTestId('create-user').click()
  await page.getByTestId('user-username').locator('input').fill(username)
  await page.getByTestId('user-display-name').locator('input').fill('SpaceGate member')
  await page.getByTestId('user-email').locator('input').fill(`${username}@example.com`)
  await page.getByTestId('user-password').locator('input').fill(password)
  await page.getByTestId('user-save').click()
  await expect(page.getByTestId('user-editor')).not.toBeVisible()

  const userContext = await browser.newContext()
  const userPage = await userContext.newPage()
  try {
    await userPage.goto(frontends.web)
    await submitLogin(userPage, username, password)
    await expect(userPage.getByTestId('logout')).toBeVisible()
    await expect(userPage.getByTestId('settings-page')).toBeVisible()

    for (const pageId of ['overview', 'gateways', 'routes', 'plugins', 'ai-queue', 'instances', 'users', 'roles', 'audit'])
      await expect(userPage.getByTestId(`nav-${pageId}`)).toHaveCount(0)
    await expect(userPage.getByTestId('nav-settings')).toBeVisible()
    await expect(userPage.getByTestId('profile-form')).toBeVisible()

    // 直接访问管理 hash 只能回落到个人设置，服务端权限仍是最终边界。
    await userPage.goto(`${frontends.web}/#/gateways`)
    await expect(userPage.getByTestId('settings-page')).toBeVisible()
    await expect(userPage).toHaveURL(/#\/settings$/)
  }
  finally {
    await userContext.close()
  }

  // 清理测试账号，避免影响后续用户列表断言。
  await page.getByTestId('users-search').locator('input').fill(username)
  await page.getByTestId('users-page').getByRole('button', { name: /查询|Search/ }).click()
  const row = page.getByRole('row').filter({ hasText: username })
  await expect(row).toBeVisible()
  await row.getByRole('button', { name: /删除|Delete/, exact: true }).click()
  await page.getByTestId('confirm-delete-user').click()
  await expect(page.getByRole('row').filter({ hasText: username })).toHaveCount(0)
})
