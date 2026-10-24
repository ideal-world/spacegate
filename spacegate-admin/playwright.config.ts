import process from 'node:process'
import { defineConfig } from '@playwright/test'
import { createE2eNotificationEnv, createE2eServerEnv, createE2eTransportRing } from './scripts/e2e-env.js'

const defaultBaseURL = 'http://127.0.0.1:4173'
const defaultServiceBaseURL = 'http://127.0.0.1:3000'
const baseURL = process.env.BASE_URL || defaultBaseURL
const primaryFrontend = 'web'
const frontendUrls = {
  web: process.env.HAI_E2E_WEB_URL,
  app: process.env.HAI_E2E_APP_URL,
  desktop: process.env.HAI_E2E_DESKTOP_URL,
}
const frontends: { target: string, name: string, url: string }[] = [
  {
    target: 'web',
    name: 'spacegate-admin-web',
    url: frontendUrls.web || 'http://127.0.0.1:4173',
  },
  {
    target: 'app',
    name: 'spacegate-admin-app',
    url: frontendUrls.app || 'http://127.0.0.1:5175',
  },
  {
    target: 'desktop',
    name: 'spacegate-admin-desktop',
    url: frontendUrls.desktop || 'http://127.0.0.1:5176',
  },
]
const serviceBaseURL = process.env.SERVICE_BASE_URL || defaultServiceBaseURL
const serviceHost = resolveHostFromBaseUrl(serviceBaseURL, '127.0.0.1')
const servicePort = resolvePortFromBaseUrl(serviceBaseURL, '3000')

function resolveHostFromBaseUrl(urlString: string, fallbackHost: string): string {
  try {
    const url = new URL(urlString)
    return url.hostname || fallbackHost
  }
  catch {
    return fallbackHost
  }
}

function resolvePortFromBaseUrl(urlString: string, fallbackPort: string): string {
  try {
    const url = new URL(urlString)
    if (url.port) {
      return url.port
    }

    return url.protocol === 'https:' ? '443' : '80'
  }
  catch {
    return fallbackPort
  }
}

export default defineConfig({
  testDir: './e2e',
  // 多个界面验收共用管理员资料与引导状态，保留串行以免互相覆盖。
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: 'list',
  timeout: 30_000,
  use: {
    baseURL,
    channel: 'chrome',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: [
    {
      name: 'spacegate-admin-serv',
      command: 'pnpm --filter spacegate-admin-serv exec node dist/index.js',
      env: createE2eServerEnv({
        ...createE2eNotificationEnv(),
        HAI_DB: JSON.stringify({ type: 'sqlite', database: `${process.env.HAI_E2E_DATA_ROOT}/fullstack.db` }),
        HAI_CACHE: JSON.stringify({ type: 'memory' }),
        HAI_STORAGE: JSON.stringify({ type: 'local', root: `${process.env.HAI_E2E_DATA_ROOT}/uploads` }),
        HAI_VECDB: JSON.stringify({ type: 'lancedb', path: `${process.env.HAI_E2E_DATA_ROOT}/vecdb` }),
        HAI_CRYPTO_TRANSPORT_SERVERKEYRING: createE2eTransportRing(),
        HOST: serviceHost,
        PORT: servicePort,
        // 仅供脚手架验收：生产启动必须由部署环境提供初始管理员密码。
        HAI_ADMIN_DEFAULT_PASSWORD: process.env.E2E_ADMIN_PASSWORD || 'Scaffold-test-Admin123!',
        HAI_CORE_NAME: process.env.HAI_CORE_NAME || 'Configured scaffold',
        HAI_CORE_VERSION: process.env.HAI_CORE_VERSION || '9.8.7',
        HAI_CORE_ENV: 'test',
        HAI_CORE_LOGGING_LEVEL: 'warn',
      }),
      url: `${serviceBaseURL}/health`,
      reuseExistingServer: false,
      timeout: 120_000,
    },
    ...frontends.map((frontend) => {
      const url = frontend.target === primaryFrontend ? baseURL : frontend.url
      return {
        name: frontend.name,
        command: `pnpm --filter ${frontend.name} exec vite preview --host ${resolveHostFromBaseUrl(url, '127.0.0.1')} --port ${resolvePortFromBaseUrl(url, '3000')} --strictPort`,
        env: createE2eServerEnv({ PUBLIC_API_BASE: serviceBaseURL }),
        url,
        reuseExistingServer: false,
        timeout: 120_000,
      }
    }),
  ],
})
