import process from 'node:process'
import { fileURLToPath } from 'node:url'
import { core } from '@h-ai/core'
import { haiOptimizeExclude, haiPrebundledDeps } from '@h-ai/ui/vite'
import { paraglideVitePlugin } from '@inlang/paraglide-js'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vitest/config'

const envDir = fileURLToPath(new URL('../../', import.meta.url))

export default defineConfig(({ mode, command }) => {
  const envResult = command === 'build' || process.env.HAI_E2E === '1' ? { success: true as const, data: {} as Record<string, string> } : core.config.loadEnvFiles({ mode, envDir })
  if (!envResult.success)
    throw new Error(envResult.error.message)
  const env = envResult.data
  Object.assign(process.env, env)
  const target = (env.PUBLIC_API_BASE || process.env.SERVICE_BASE_URL || 'http://127.0.0.1:3000').replace(/\/api\/v1\/?$/, '')
  const proxy = { '/api': { target, changeOrigin: true } }
  return {
    envDir: false,
    server: { proxy },
    preview: { proxy },
    build: {
      rolldownOptions: {
        output: {
          codeSplitting: {
            groups: [
              { name: 'vendor-ui', test: /node_modules\/(?:@h-ai\/ui|bits-ui|svelte)\// },
              { name: 'vendor-crypto', test: /node_modules\/(?:@h-ai\/(?:core|crypto)|sm-crypto(?:-v2)?)\// },
              { name: 'vendor-api', test: /node_modules\/(?:@orpc\/|zod\/)/ },
            ],
          },
        },
      },
    },
    plugins: [
      svelte(),
      paraglideVitePlugin({
        project: './project.inlang',
        outdir: './src/lib/paraglide',
        strategy: ['cookie', 'baseLocale'],
      }),
      tailwindcss(),
    ],
    optimizeDeps: {
      exclude: [...haiOptimizeExclude],
      include: [...haiPrebundledDeps],
    },
    envPrefix: ['VITE_', 'PUBLIC_'],
    ssr: {
      noExternal: ['@h-ai/ui', /spacegate-admin-shared/],
    },
    test: {
      environment: 'node',
      include: ['tests/**/*.test.ts'],
    },
  }
})
