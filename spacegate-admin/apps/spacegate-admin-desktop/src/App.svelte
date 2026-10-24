<script lang="ts">
  import type { ManagementApiOptions } from 'spacegate-admin-shared'
  import { apiClient } from '@h-ai/api-client'
  import { AdminWorkspace } from 'spacegate-admin-shared'
  import { setLocale } from './lib/paraglide/runtime.js'

  const native = '__TAURI_INTERNALS__' in globalThis
  const apiOptions: ManagementApiOptions = {
    clientName: 'spacegate-admin-desktop',
    ...(native ? { requireApiBase: true, tokenStorage: apiClient.tokenStorage.memory(), headers: { 'X-HAI-Token-Transport': 'body' } } : {}),
  }

  function syncLocale(locale: string) {
    setLocale(locale as Parameters<typeof setLocale>[0], { reload: false })
  }
</script>

<AdminWorkspace platform="desktop" {apiOptions} onLocaleChange={syncLocale} />
