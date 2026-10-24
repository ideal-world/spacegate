<script lang="ts">
  import type { ManagementApiOptions } from 'spacegate-admin-shared'
  import { capacitor, createCapacitorTokenStorage } from '@h-ai/capacitor'
  import { core } from '@h-ai/core'
  import { Alert } from '@h-ai/ui/components/compounds/index'
  import { Button, Spinner } from '@h-ai/ui/components/primitives/index'
  import { onMount } from 'svelte'
  import { AdminWorkspace, messages } from 'spacegate-admin-shared'
  import { initCapacitor } from './lib/capacitor.js'
  import { setLocale } from './lib/paraglide/runtime.js'

  let apiOptions = $state<ManagementApiOptions | null>(null)
  let startupFailed = $state(false)
  function syncLocale(locale: string) {
    setLocale(locale as Parameters<typeof setLocale>[0], { reload: false })
  }
  async function initialize() {
    startupFailed = false
    const result = await initCapacitor()
    if (!result.success) {
      startupFailed = true
      return
    }
    apiOptions = {
      clientName: 'spacegate-admin-app',
      ...(capacitor.isNative() ? { requireApiBase: true, tokenStorage: createCapacitorTokenStorage(), headers: { 'X-HAI-Token-Transport': 'body' } } : {}),
    }
  }
  onMount(() => {
    void initialize()
    return () => {
      void capacitor.close().then((closed) => {
        if (!closed.success)
          core.logger.warn('Native listener cleanup failed; retry on next initialization')
      })
    }
  })
</script>

{#if startupFailed}
  <div class="mx-auto flex min-h-dvh max-w-md flex-col justify-center gap-4 p-6">
    <Alert variant="error">{messages.operation_failed()}</Alert>
    <Button onclick={() => void initialize()}>{messages.home_refresh()}</Button>
  </div>
{:else if apiOptions}
  <AdminWorkspace platform="app" {apiOptions} onLocaleChange={syncLocale} />
{:else}
  <div class="flex min-h-dvh items-center justify-center"><Spinner /></div>
{/if}
