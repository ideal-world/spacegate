import type { Page } from '@playwright/test'
import { Buffer } from 'node:buffer'
import { randomUUID } from 'node:crypto'
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import { apiContract } from '@h-ai/api-contract'
import { expect, test } from '@playwright/test'
import { spacegateAdminContract } from 'spacegate-admin-contract'

const apiBaseUrl = process.env.SERVICE_BASE_URL || 'http://127.0.0.1:3000'
const apiPrefix = '/api/v1'
const adminPassword = process.env.E2E_ADMIN_PASSWORD || 'Scaffold-test-Admin123!'
const configuredName = process.env.HAI_CORE_NAME || 'Configured scaffold'
const configuredVersion = process.env.HAI_CORE_VERSION || '9.8.7'
const configuredEnv = 'test'

// 测试专用的一像素 PNG；上传仍使用完整前后端加密链路。
const avatar = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aSxkAAAAASUVORK5CYII=', 'base64')

async function capture(page: Page, name: string, fallback: string) {
  const evidence = process.env.HAI_AUTH_EVIDENCE_DIR
  if (evidence)
    await mkdir(evidence, { recursive: true })
  await expect(page.getByTestId('guide-loading')).toHaveCount(0)
  await page.evaluate(() => document.fonts.ready)
  await page.screenshot({ path: evidence ? path.join(evidence, name) : fallback, fullPage: true, animations: 'disabled' })
}

async function submitLogin(page: Page, username: string, password: string) {
  await expect(page.getByTestId('login-form')).toBeVisible()
  await page.locator('#login-username').fill(username)
  await page.getByTestId('login-form').locator('input[type="password"]').fill(password)
  await page.getByTestId('login-form').getByRole('button', { name: /登录|Sign in|Login|Log in/i }).click()
}

async function login(page: Page, username: string, password: string, keepGuide = false) {
  await submitLogin(page, username, password)
  await expect(page.getByTestId('logout')).toBeVisible()
  if (!keepGuide && await page.getByTestId('guide-later').isVisible())
    await page.getByTestId('guide-later').click()
}

async function userEditor(page: Page, username: string) {
  await page.getByTestId('users-search').locator('input').fill(username)
  await page.getByTestId('users-page').getByRole('button', { name: /查询|Search/ }).click()
  const row = page.getByRole('row').filter({ hasText: username })
  await expect(row).toBeVisible()
  await row.getByRole('button', { name: /编辑|Edit/, exact: true }).click()
  return row
}

/** 通过管理员用户页创建账号；公开注册关闭后，这是浏览器测试的账号入口。 */
async function createAccount(page: Page, username: string, password: string, email?: string) {
  await page.getByTestId('create-user').click()
  await page.getByTestId('user-username').locator('input').fill(username)
  await page.getByTestId('user-password').locator('input').fill(password)
  if (email)
    await page.getByTestId('user-email').locator('input').fill(email)
  await page.getByTestId('user-save').click()
  await expect(page.getByTestId('user-editor')).not.toBeVisible()
}

test('service health endpoint is ready', async ({ request }) => {
  const response = await request.get(`${apiBaseUrl}/health`)
  await expect(response).toBeOK()
})

test('service rejects plaintext business requests', async ({ request }) => {
  const response = await request.post(`${apiBaseUrl}${apiPrefix}${apiContract.pathOf(spacegateAdminContract.app.echo)}`, { data: { message: 'hello e2e' } })
  expect(response.status()).toBe(400)
})

test('hash routing and responsive workspace preserve usable content width', async ({ page }, testInfo) => {
  await page.goto('/#/roles?source=direct')
  await login(page, 'admin', adminPassword)
  await expect(page.getByTestId('roles-page')).toBeVisible()
  await expect(page).toHaveURL(/#\/roles\?source=direct$/)
  await page.reload()
  await expect(page.getByTestId('roles-page')).toBeVisible()
  await expect(page.locator('header')).toBeVisible()
  await expect(page.locator('footer')).toBeVisible()
  for (const width of [1440, 1920, 2560, 3840, 390]) {
    await page.setViewportSize({ width, height: 960 })
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    const main = await page.locator('main').boundingBox()
    const content = await page.getByTestId('roles-page').boundingBox()
    expect(content!.width).toBeGreaterThan(main!.width - 80)
    if (width === 1920)
      await capture(page, 'spacegate-admin-wide.png', testInfo.outputPath('wide-1920.png'))
  }
  await page.setViewportSize({ width: 1440, height: 960 })
  await page.getByTestId('nav-settings').click()
  await expect(page).toHaveURL(/#\/settings$/)
  await page.goBack()
  await expect(page.getByTestId('roles-page')).toBeVisible()
  await page.goForward()
  await expect(page.getByTestId('profile-form')).toBeVisible()
  await page.goto('/#/unknown-page')
  await expect(page).toHaveURL(/#\/overview$/)
  await expect(page.getByTestId('overview-page')).toBeVisible()
})

test('branded authentication, profile layout and blur validation work across screen sizes', async ({ page }, testInfo) => {
  test.setTimeout(60_000)
  await page.setViewportSize({ width: 1440, height: 960 })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: configuredName, exact: true })).toBeVisible()
  await expect(page.locator('header')).toBeVisible()
  await expect(page.locator('footer')).toBeVisible()
  await capture(page, 'spacegate-admin-login-desktop.png', testInfo.outputPath('login-desktop.png'))
  // 公开注册默认关闭：登录页不得再暴露注册入口或注册表单。
  await expect(page.getByRole('button', { name: /立即注册|Register now/, exact: true })).toHaveCount(0)
  await expect(page.locator('.register-form')).toHaveCount(0)
  await page.setViewportSize({ width: 390, height: 844 })
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await page.reload()
  await expect(page.getByTestId('login-form')).toBeVisible()
  await capture(page, 'spacegate-admin-login-mobile.png', testInfo.outputPath('login-mobile.png'))
  await page.setViewportSize({ width: 1440, height: 960 })
  await login(page, 'admin', adminPassword)
  await page.getByTestId('nav-settings').click()
  const profile = page.getByTestId('profile-form')
  await expect(profile).toBeVisible()
  const email = profile.locator('input[name="email"]')
  await email.fill('invalid-email')
  await email.press('Tab')
  await expect(email).toHaveAttribute('aria-invalid', 'true')
  await capture(page, 'spacegate-admin-profile-blur-error.png', testInfo.outputPath('profile-blur-error.png'))
  await email.fill('')
  await expect(email).not.toHaveAttribute('aria-invalid')
  await capture(page, 'spacegate-admin-settings-desktop.png', testInfo.outputPath('settings-desktop.png'))
  for (const width of [768, 390]) {
    await page.setViewportSize({ width, height: 960 })
    const fields = await profile.locator('.hai-form-field').evaluateAll(elements => elements.map((field) => {
      const label = field.querySelector('label')!.getBoundingClientRect()
      const control = field.querySelector('input')!.getBoundingClientRect()
      return { labelBottom: label.bottom, controlTop: control.top, controlRight: control.right }
    }))
    for (const field of fields) {
      expect(field.labelBottom).toBeLessThanOrEqual(field.controlTop)
      expect(field.controlRight).toBeLessThanOrEqual(width)
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await capture(page, `spacegate-admin-settings-${width}.png`, testInfo.outputPath(`settings-${width}.png`))
  }
  await page.getByTestId('password-form').scrollIntoViewIfNeeded()
  await capture(page, 'spacegate-admin-password-mobile.png', testInfo.outputPath('password-mobile.png'))
  await page.setViewportSize({ width: 1440, height: 960 })
  await page.getByTestId('nav-users').click()
  await page.getByTestId('create-user').click()
  const username = page.getByTestId('user-username').locator('input')
  await username.focus()
  await username.press('Tab')
  await expect(username).toHaveAttribute('aria-invalid', 'true')
  await username.fill('a')
  await expect(username).toHaveAttribute('aria-invalid', 'true')
  await page.getByTestId('user-password').locator('input').fill('short')
  await page.getByTestId('user-password').locator('input').press('Tab')
  await expect(page.getByTestId('user-password').locator('input')).toHaveAttribute('aria-invalid', 'true')
  await capture(page, 'spacegate-admin-create-blur-error.png', testInfo.outputPath('create-blur-error.png'))
  await page.getByTestId('user-editor').getByRole('button', { name: /取消|Cancel/, exact: true }).click()
  await page.getByTestId('create-user').click()
  await expect(page.getByTestId('user-editor').locator('[aria-invalid="true"]')).toHaveCount(0)
  await page.getByTestId('user-editor').getByRole('button', { name: /取消|Cancel/, exact: true }).click()
  await userEditor(page, 'admin')
  const editEmail = page.getByTestId('user-email').locator('input')
  await editEmail.fill('invalid-email')
  await editEmail.press('Tab')
  await expect(editEmail).toHaveAttribute('aria-invalid', 'true')
  await capture(page, 'spacegate-admin-edit-blur-error.png', testInfo.outputPath('edit-blur-error.png'))
  await editEmail.fill('admin@example.com')
  await expect(editEmail).not.toHaveAttribute('aria-invalid')
})

test('first-use tour, help search and settings navigation work on desktop and mobile', async ({ page }, testInfo) => {
  await page.goto('/')
  await login(page, 'admin', adminPassword, true)
  await expect(page.getByTestId('guide-offer')).toBeVisible()
  await page.getByTestId('guide-start').click()
  const popover = page.locator('.hai-guide-popover')
  await expect(popover).toBeVisible()
  await capture(page, 'spacegate-admin-guide-desktop.png', testInfo.outputPath('guide-desktop.png'))
  for (let step = 1; step < 3; step++) {
    await popover.locator('.driver-popover-next-btn').click()
    await expect(popover.locator('.driver-popover-progress-text')).toContainText(String(step + 1))
  }
  await popover.locator('.driver-popover-next-btn').click()
  await expect(popover).toHaveCount(0)
  await page.reload()
  await expect(page.getByTestId('admin-help')).toBeVisible()
  await expect(page.getByTestId('guide-offer')).toHaveCount(0)
  await page.getByTestId('admin-help').click()
  const profileTitle = (await page.getByTestId('guide-article-profile').textContent())!.trim()
  await page.getByTestId('guide-search').locator('input').fill(profileTitle)
  await expect(page.getByTestId('guide-article-profile')).toBeVisible()
  await expect(page.getByTestId('guide-article-overview')).toHaveCount(0)
  await page.getByTestId('guide-article-start').click()
  await expect(page.getByTestId('settings-page')).toBeVisible()
  await expect(popover).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(popover).toHaveCount(0)
  await page.setViewportSize({ width: 390, height: 844 })
  await page.getByTestId('admin-help').click()
  await expect(page.getByTestId('help-center')).toBeVisible()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await capture(page, 'spacegate-admin-guide-help-mobile.png', testInfo.outputPath('guide-help-mobile.png'))
  await page.getByTestId('guide-tour-overview').click()
  await expect(popover.locator('.driver-popover-next-btn')).toBeEnabled()
  // 下一步未渲染时仍能通过宿主提示取消，不能被引擎遮罩阻止。
  await page.locator('[data-guide="admin-profile"]').evaluate(element => element.removeAttribute('data-guide'))
  await popover.locator('.driver-popover-next-btn').click()
  await expect(page.getByTestId('guide-loading')).toBeVisible()
  await page.getByTestId('guide-cancel').click()
  await expect(popover).toHaveCount(0)
  await expect(page.getByTestId('guide-error')).toHaveCount(0)
})

// 所有已生成的浏览器前端都验证共享引导，不能用 Web 的成功代替 App/Desktop 预览。
const additionalFrontends = [
  {
    target: 'web',
    url: process.env['HAI_E2E_web_URL'.toUpperCase()] || 'http://127.0.0.1:4173',
  },
  {
    target: 'app',
    url: process.env['HAI_E2E_app_URL'.toUpperCase()] || 'http://127.0.0.1:5175',
  },
  {
    target: 'desktop',
    url: process.env['HAI_E2E_desktop_URL'.toUpperCase()] || 'http://127.0.0.1:5176',
  },
].filter(frontend => frontend.target !== 'web')

for (const frontend of additionalFrontends) {
  test(`${frontend.target} preview supports login, tour, help and profile navigation`, async ({ browser }, testInfo) => {
    const page = await browser.newPage({ viewport: frontend.target === 'app' ? { width: 390, height: 844 } : { width: 1440, height: 960 } })
    const pageErrors: string[] = []
    page.on('pageerror', error => pageErrors.push(error.message))
    try {
      await page.goto(frontend.url)
      await expect(page.getByTestId('login-form')).toBeVisible()
      await capture(page, `spacegate-admin-${frontend.target}-login.png`, testInfo.outputPath(`${frontend.target}-login.png`))
      await login(page, 'admin', adminPassword, true)
      await expect(page.getByTestId('guide-offer')).toBeVisible()
      await page.getByTestId('guide-start').click()
      const popover = page.locator('.hai-guide-popover')
      await expect(popover).toBeVisible()
      await capture(page, `spacegate-admin-${frontend.target}-guide.png`, testInfo.outputPath(`${frontend.target}-guide.png`))
      await page.keyboard.press('Escape')
      await expect(popover).toHaveCount(0)
      await page.getByTestId('admin-help').click()
      await expect(page.getByTestId('help-center')).toBeVisible()
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
      await capture(page, `spacegate-admin-${frontend.target}-help.png`, testInfo.outputPath(`${frontend.target}-help.png`))
      await page.getByTestId('guide-article-profile').click()
      await page.getByTestId('guide-article-start').click()
      await expect(page.getByTestId('settings-page')).toBeVisible()
      await expect(popover).toBeVisible()
      await page.keyboard.press('Escape')
      await expect(popover).toHaveCount(0)
      await capture(page, `spacegate-admin-${frontend.target}-profile.png`, testInfo.outputPath(`${frontend.target}-profile.png`))
      expect(pageErrors).toEqual([])
    }
    finally {
      await page.close()
    }
  })
}

test('public registration is closed and administrator-created accounts sign in', async ({ page }) => {
  test.setTimeout(90_000)
  const evidence = process.env.HAI_AUTH_EVIDENCE_DIR || path.resolve('test-results/auth-screenshots')
  await mkdir(evidence, { recursive: true })
  const username = `member${randomUUID().replaceAll('-', '').slice(0, 12)}`
  const password = 'SignupPassword123'
  await page.goto('/')
  await expect(page.getByTestId('login-form')).toBeVisible()
  // 公开注册默认关闭，登录页不渲染注册入口；账号由管理员创建。
  await expect(page.getByRole('button', { name: /立即注册|Register now/i })).toHaveCount(0)
  await expect(page.locator('.register-form')).toHaveCount(0)
  await page.screenshot({ path: path.join(evidence, 'spacegate-admin-login.png'), fullPage: true })
  await login(page, 'admin', adminPassword)
  await page.getByTestId('nav-users').click()
  await createAccount(page, username, password, `${username}@example.com`)
  await expect(page.getByRole('row').filter({ hasText: username })).toBeVisible()
  await page.getByTestId('logout').click()
  await login(page, username, password)
  await expect(page.getByTestId('settings-page')).toBeVisible()
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await page.reload()
  await expect(page.getByTestId('settings-page')).toBeVisible()
})

test('IAM email OTP and password reset work for administrator-created accounts', async ({ page }) => {
  test.skip(!process.env.IAM_NOTIFICATION_URL, 'IAM notification fixture is not configured')
  test.setTimeout(120_000)
  const evidence = process.env.HAI_AUTH_EVIDENCE_DIR || path.resolve('test-results/auth-screenshots')
  await mkdir(evidence, { recursive: true })
  const username = `otp${randomUUID().replaceAll('-', '').slice(0, 12)}`
  const email = `${username}@example.com`
  const password = 'SignupPassword123'
  await page.goto('/')
  await login(page, 'admin', adminPassword)
  await page.getByTestId('nav-users').click()
  await createAccount(page, username, password, email)
  await expect(page.getByRole('row').filter({ hasText: username })).toBeVisible()
  await page.getByTestId('logout').click()
  await page.getByRole('button', { name: /邮件验证码|Email code/i, exact: true }).click()
  await page.screenshot({ path: path.join(evidence, 'spacegate-admin-email.png'), fullPage: true })
  await page.locator('#login-username').fill(email)
  await page.getByRole('button', { name: /发送验证码|Send code/i, exact: true }).click()
  await expect(page.getByRole('button', { name: /秒后重发|Resend in/ })).toBeDisabled()
  async function received(template: string) {
    const response = await page.request.get(`${process.env.IAM_NOTIFICATION_URL}/messages?to=${encodeURIComponent(email)}`)
    const messages = await response.json() as { template: string, vars: Record<string, string> }[]
    return messages.findLast(item => item.template === template)?.vars
  }
  const otp = await received('otp_email')
  expect(Boolean(otp?.code)).toBe(true)
  await page.locator('#login-code').fill(otp?.code ?? '')
  await page.locator('button[type="submit"]').click()
  await expect(page.getByTestId('settings-page')).toBeVisible()
  await page.getByTestId('logout').click()
  await page.getByRole('button', { name: /忘记密码|Forgot password/i }).click()
  await page.screenshot({ path: path.join(evidence, 'spacegate-admin-forgot.png'), fullPage: true })
  await page.locator('input[type="email"]').fill(email)
  await page.locator('button[type="submit"]').click()
  await expect(page.locator('.alert-success')).toBeVisible()
  const reset = await received('password_reset')
  expect(Boolean(reset?.token)).toBe(true)
  await page.getByRole('button', { name: /已有重置令牌|I have a reset token/i }).click()
  await page.screenshot({ path: path.join(evidence, 'spacegate-admin-reset.png'), fullPage: true })
  await page.goto(`/?token=${encodeURIComponent(reset?.token ?? '')}`)
  await expect(page.locator('input[type="password"]').nth(0)).toBeVisible()
  await page.locator('input[type="password"]').nth(0).fill('FullstackReset456')
  await page.locator('input[type="password"]').nth(1).fill('FullstackReset456')
  await page.locator('button[type="submit"]').click()
  await expect(page.locator('.alert-success')).toBeVisible()
  await page.getByRole('button', { name: /^(登录|Sign in|Login)$/i }).click()
  await login(page, username, 'FullstackReset456')
  await expect(page.getByTestId('settings-page')).toBeVisible()
  expect(new URL(page.url()).searchParams.has('token')).toBe(false)
  await page.reload()
  await expect(page.getByTestId('settings-page')).toBeVisible()
})

test('frontend displays configured core metadata and restores the administrator session', async ({ page }, testInfo) => {
  await page.goto('/')
  await expect(page.getByTestId('app-name')).toHaveText(configuredName)
  await expect(page).toHaveTitle(configuredName)
  await expect(page.getByTestId('app-version')).toHaveText(`v${configuredVersion}`)
  await expect(page.getByTestId('app-environment')).toHaveText(configuredEnv)
  await login(page, 'admin', adminPassword)
  await expect(page.getByTestId('overview-page')).toBeVisible()
  await page.reload()
  await expect(page.getByTestId('overview-page')).toBeVisible()
  await expect(page.getByTestId('nav-roles')).toBeVisible()
  await expect(page.getByTestId('nav-audit')).toBeVisible()
  const evidence = process.env.HAI_AUTH_EVIDENCE_DIR
  if (evidence)
    await mkdir(evidence, { recursive: true })
  await page.screenshot({ path: evidence ? path.join(evidence, 'spacegate-admin-users.png') : testInfo.outputPath('users.png'), fullPage: true })
  await page.getByTestId('logout').click()
  await expect(page.getByTestId('login-form')).toBeVisible()
})

test('audit filters align and fit desktop, tablet and narrow mobile screens', async ({ page }, testInfo) => {
  await page.goto('/')
  await login(page, 'admin', adminPassword)
  await page.getByTestId('nav-audit').click()
  await expect(page.getByTestId('audit-page')).toBeVisible()
  for (const width of [1440, 960, 390, 320]) {
    await page.setViewportSize({ width, height: 900 })
    // 等待移动侧栏收起，避免过渡帧遮挡内容并污染截图。
    if (width < 1024)
      await expect.poll(() => page.locator('aside').evaluate(sidebar => sidebar.getBoundingClientRect().right)).toBeLessThanOrEqual(1)
    const controls = await page.getByTestId('audit-page').locator('form').evaluate(form => [
      form.querySelector('#audit-action')?.parentElement,
      form.querySelector('#audit-resource')?.parentElement,
      form.querySelector('#audit-start'),
      form.querySelector('#audit-end'),
      form.querySelector('button[type="submit"]'),
    ].map((element) => {
      if (!element)
        throw new Error('Audit filter control is missing')
      const rect = element.getBoundingClientRect()
      return { top: rect.top, bottom: rect.bottom, height: rect.height }
    }))
    for (const control of controls)
      expect(control.height).toBeCloseTo(40, 0)
    if (width >= 1280) {
      expect(Math.max(...controls.map(control => control.top)) - Math.min(...controls.map(control => control.top))).toBeLessThan(1)
      expect(Math.max(...controls.map(control => control.bottom)) - Math.min(...controls.map(control => control.bottom))).toBeLessThan(1)
    }
    else if (width >= 640) {
      expect(controls[0]!.top).toBeCloseTo(controls[1]!.top, 0)
      expect(controls[2]!.top).toBeCloseTo(controls[3]!.top, 0)
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    if (width === 1440 || width === 390) {
      const evidence = process.env.HAI_AUTH_EVIDENCE_DIR
      if (evidence)
        await mkdir(evidence, { recursive: true })
      const file = `spacegate-admin-audit-${width}.png`
      await page.screenshot({ path: evidence ? path.join(evidence, file) : testInfo.outputPath(file), fullPage: true, animations: 'disabled' })
    }
  }
})

test('administrator and user complete account, avatar, preference, password and audit workflows', async ({ page, browser }) => {
  test.setTimeout(120_000)
  const username = `member-${randomUUID().slice(0, 8)}`
  const password = 'User-test-Pass123!'
  const newPassword = 'Changed-test-Pass123!'
  const encryptedRequests: string[] = []
  const encryptedResponses: string[] = []
  page.on('request', (request) => {
    if (request.url().includes(`${apiPrefix}/`) && !request.url().includes('/_hai/') && request.postData()) {
      expect(request.headers()['x-encrypted']).toBe('true')
      expect(request.postData()).not.toContain('"password"')
      expect(request.postData()).not.toContain(adminPassword)
      encryptedRequests.push(request.url())
    }
  })
  page.on('response', (response) => {
    if (response.request().method() !== 'OPTIONS' && response.url().includes(`${apiPrefix}/`) && !response.url().includes('/_hai/') && !response.url().includes('/assets/')) {
      expect(response.headers()['x-encrypted']).toBe('true')
      encryptedResponses.push(response.url())
    }
  })
  await page.goto('/')
  await login(page, 'admin', adminPassword)
  await expect(page.getByTestId('overview-page')).toBeVisible()
  await page.getByTestId('nav-users').click()
  await expect(page.getByTestId('users-page')).toBeVisible()
  await page.getByTestId('create-user').click()
  await expect(page.getByRole('dialog', { name: /新增用户|New user|Create user|Add user/ })).toBeVisible()
  await expect(page.getByTestId('user-username').locator('input')).toBeFocused()
  await page.getByTestId('user-username').locator('input').fill(username)
  await page.getByTestId('user-display-name').locator('input').fill('Test member')
  await page.getByTestId('user-email').locator('input').fill(`${username}@example.com`)
  await page.getByTestId('user-password').locator('input').fill(password)
  await page.getByTestId('user-save').click()
  await expect(page.getByTestId('user-editor')).not.toBeVisible()
  const userRow = page.getByRole('row').filter({ hasText: username })
  await expect(userRow).toBeVisible()
  await userRow.getByTestId('user-avatar-input').setInputFiles({ name: 'avatar.png', mimeType: 'image/png', buffer: avatar })
  await expect(userRow.locator('img')).toHaveAttribute('src', /\/assets\/avatars\//)
  await expect.poll(() => userRow.locator('img').evaluate((image: HTMLImageElement) => image.complete && image.naturalWidth > 0)).toBe(true)
  await page.getByTestId('nav-roles').click()
  await expect(page.getByTestId('roles-page')).toBeVisible()
  await page.getByTestId('roles-page').getByRole('button', { name: /编辑说明|Edit description/ }).first().click()
  await page.getByTestId('role-description').locator('textarea').fill('Workspace administration')
  await page.getByRole('dialog').getByRole('button', { name: /保存|Save/, exact: true }).focus()
  await page.keyboard.press('Tab')
  await expect(page.getByRole('dialog').getByRole('button', { name: /关闭|Close/, exact: true })).toBeFocused()
  await page.keyboard.press('Shift+Tab')
  await expect(page.getByRole('dialog').getByRole('button', { name: /保存|Save/, exact: true })).toBeFocused()
  await page.getByRole('dialog').getByRole('button', { name: /保存|Save/, exact: true }).click()
  await expect(page.getByTestId('roles-page')).toContainText('Workspace administration')

  const userContext = await browser.newContext()
  const userPage = await userContext.newPage()
  try {
    await userPage.goto('/')
    await login(userPage, username, password)
    await expect(userPage.getByTestId('settings-page')).toBeVisible()
    await expect(userPage.getByTestId('nav-users')).toHaveCount(0)
    await expect(userPage.getByTestId('nav-roles')).toHaveCount(0)
    await expect(userPage.getByTestId('nav-audit')).toHaveCount(0)
    await userPage.getByTestId('profile-form').locator('input[name="displayName"]').fill('Updated member')
    await userPage.getByTestId('profile-form').locator('input[name="email"]').fill('')
    await userPage.getByTestId('profile-form').getByRole('button', { name: /保存|Save/, exact: true }).click()
    await expect(userPage.getByTestId('notice')).toBeVisible()
    await userPage.getByTestId('profile-form').locator('input[type="file"]').setInputFiles({ name: 'personal.png', mimeType: 'image/png', buffer: avatar })
    await expect(userPage.getByTestId('notice')).toHaveText(/头像已更新|Avatar updated/)
    await userPage.getByTestId('profile-language').click()
    await userPage.getByRole('option', { name: 'English', exact: true }).click()
    await expect(userPage.getByTestId('nav-settings')).toHaveText('Personal settings')
    await expect(userPage.locator('html')).toHaveAttribute('lang', 'en-US')
    const themes = ['light', 'dark']
    const selectedTheme = themes.find(theme => theme !== 'light') ?? themes[0]
    await userPage.getByTestId('profile-theme').click()
    await userPage.getByRole('option').nth(themes.indexOf(selectedTheme)).click()
    await expect(userPage.locator('html')).toHaveAttribute('data-theme', selectedTheme)
    await userPage.reload()
    await expect(userPage.getByTestId('settings-page')).toBeVisible()
    await expect(userPage.getByTestId('nav-settings')).toHaveText('Personal settings')
    await expect(userPage.locator('html')).toHaveAttribute('data-theme', selectedTheme)
    await expect(userPage.getByTestId('profile-form').locator('input[name="displayName"]')).toHaveValue('Updated member')
    await expect(userPage.getByTestId('profile-form').locator('input[name="email"]')).toHaveValue('')
    await expect.poll(() => userPage.getByTestId('profile-form').locator('img').evaluate((image: HTMLImageElement) => image.complete && image.naturalWidth > 0)).toBe(true)
    await userPage.getByTestId('profile-language').click()
    await userPage.getByRole('option', { name: '日本語', exact: true }).click()
    await expect(userPage.getByTestId('nav-settings')).toHaveText('個人設定')
    await expect(userPage.locator('html')).toHaveAttribute('lang', 'ja-JP')
    await userPage.reload()
    await expect(userPage.getByTestId('nav-settings')).toHaveText('個人設定')
    await expect(userPage.locator('html')).toHaveAttribute('lang', 'ja-JP')
    await userPage.getByTestId('profile-language').click()
    await userPage.getByRole('option', { name: 'English', exact: true }).click()
    await expect(userPage.getByTestId('nav-settings')).toHaveText('Personal settings')
    await userPage.setViewportSize({ width: 390, height: 844 })
    await expect.poll(() => userPage.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true)
    await userPage.getByTestId('mobile-menu').click()
    await expect(userPage.getByTestId('nav-settings')).toBeVisible()
    await userPage.getByTestId('nav-settings').click()
    const passwordInputs = userPage.getByTestId('password-form').locator('input[type="password"]')
    await passwordInputs.nth(0).fill(password)
    await passwordInputs.nth(1).fill(password)
    await passwordInputs.nth(2).fill(password)
    await userPage.getByTestId('password-form').getByRole('button', { name: /Change password/i }).click()
    await expect(userPage.getByTestId('settings-page')).toBeVisible()
    await expect(userPage.getByTestId('error')).toContainText(/检查|check/i)
    await passwordInputs.nth(1).fill(newPassword)
    await passwordInputs.nth(2).fill(newPassword)
    await userPage.getByTestId('password-form').getByRole('button', { name: /Change password/i }).click()
    await expect(userPage.getByTestId('login-form')).toBeVisible()
    await submitLogin(userPage, username, password)
    await expect(userPage.getByTestId('login-form')).toContainText(/不正确|incorrect/i)
    await login(userPage, username, newPassword)
    await expect(userPage.getByTestId('settings-page')).toBeVisible()

    await page.getByTestId('nav-users').click()
    await userEditor(page, username)
    await page.getByTestId('user-editor').locator('input[type="checkbox"]').uncheck()
    await page.getByTestId('user-save').click()
    await expect(page.getByTestId('user-editor')).not.toBeVisible()
    await userPage.getByTestId('profile-form').getByRole('button', { name: 'Save', exact: true }).click()
    await expect(userPage.getByTestId('login-form')).toBeVisible()
    await expect(userPage.getByTestId('login-form')).toContainText(/expired|过期/)
  }
  finally {
    await userContext.close()
  }

  await page.getByRole('row').filter({ hasText: username }).getByRole('button', { name: /删除|Delete/, exact: true }).click()
  await page.getByTestId('confirm-delete-user').click()
  await expect(page.getByRole('row').filter({ hasText: username })).toHaveCount(0)
  await page.getByTestId('nav-audit').click()
  await page.getByTestId('audit-resource').locator('input').fill('users')
  await page.getByTestId('audit-page').getByRole('button', { name: /查询|Search/ }).click()
  await expect(page.getByTestId('audit-page')).toContainText('delete')
  await page.getByTestId('audit-page').getByRole('button', { name: /详情|Details/ }).first().click()
  await expect(page.getByRole('dialog')).toContainText(/users/)
  expect(encryptedRequests.length).toBeGreaterThan(5)
  expect(encryptedResponses.length).toBeGreaterThan(5)
})
