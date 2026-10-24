import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import { getLocale, labelsFor, setLocale, t } from '../miniprogram/lib/i18n'

const APP_DIR = fileURLToPath(new URL('..', import.meta.url))
const MINIPROGRAM_DIR = path.join(APP_DIR, 'miniprogram')

const REQUIRED_PAGES = [
  'pages/login/index',
  'pages/overview/index',
  'pages/gateways/index',
  'pages/routes/index',
  'pages/plugins/index',
  'pages/ai-queue/index',
  'pages/instances/index',
  'pages/profile/index',
]

interface AppManifest {
  pages: string[]
  sitemapLocation?: string
  window?: Record<string, unknown>
}

interface ProjectConfig {
  appid: string
  compileType: string
  miniprogramRoot: string
  useCompilerPlugins?: string[]
}

interface RegisteredPage {
  data: Record<string, unknown>
  onShow?: () => void
}

const registered: RegisteredPage[] = []

function installPageRecorder(): void {
  ;(globalThis as unknown as { Page: (options: RegisteredPage) => void }).Page = (options) => {
    registered.push(options)
  }
}

afterEach(() => {
  registered.length = 0
  Reflect.deleteProperty(globalThis, 'Page')
})

describe('miniapp project manifest', () => {
  it('declares every page with ts/wxml/wxss/json files', async () => {
    const manifest = JSON.parse(await readFile(path.join(MINIPROGRAM_DIR, 'app.json'), 'utf8')) as AppManifest
    for (const page of manifest.pages) {
      for (const extension of ['.ts', '.wxml', '.wxss', '.json'])
        await expect(readFile(path.join(MINIPROGRAM_DIR, `${page}${extension}`), 'utf8')).resolves.toBeTypeOf('string')
    }
    for (const page of REQUIRED_PAGES)
      expect(manifest.pages).toContain(page)
    expect(manifest.sitemapLocation).toBe('sitemap.json')
  })

  it('registers every page module with data and an onShow guard', async () => {
    installPageRecorder()
    await import('../miniprogram/pages/login/index')
    await import('../miniprogram/pages/overview/index')
    await import('../miniprogram/pages/gateways/index')
    await import('../miniprogram/pages/routes/index')
    await import('../miniprogram/pages/plugins/index')
    await import('../miniprogram/pages/ai-queue/index')
    await import('../miniprogram/pages/instances/index')
    await import('../miniprogram/pages/profile/index')
    expect(registered).toHaveLength(REQUIRED_PAGES.length)
    for (const page of registered) {
      expect(page.data).toBeTypeOf('object')
      expect(page.onShow).toBeTypeOf('function')
      expect(page.data.labels).toBeTypeOf('object')
    }
  })

  it('uses a real TypeScript miniprogram project configuration', async () => {
    const config = JSON.parse(await readFile(path.join(APP_DIR, 'project.config.json'), 'utf8')) as ProjectConfig
    expect(config.compileType).toBe('miniprogram')
    expect(config.miniprogramRoot).toBe('miniprogram/')
    expect(config.useCompilerPlugins).toContain('typescript')
    expect(config.appid.length).toBeGreaterThan(0)
  })

  it('keeps zh-CN as default and switches languages through the shared translation overlay', () => {
    setLocale('zh-CN')
    expect(getLocale()).toBe('zh-CN')
    expect(t('mini_nav_overview')).toBe('概览')
    expect(t('mini_health_summary', { online: 2, total: 3 })).toBe('在线 2 / 共 3')
    setLocale('en-US')
    expect(t('mini_nav_overview')).toBe('Overview')
    expect(labelsFor(['save', 'cancel']).save).toBe('Save')
    setLocale('zh-CN')
  })
})
