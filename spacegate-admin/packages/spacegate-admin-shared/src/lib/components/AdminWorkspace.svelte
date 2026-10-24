<script lang='ts'>
  import type { Component } from 'svelte'
  import type { ManagementApiOptions } from '../api/api-client.js'
  import { Alert, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Button } from '@h-ai/ui/components/primitives/index'
  import { AuthPanel } from '@h-ai/ui/components/scenes/iam/index'
  import { GuideHost } from '@h-ai/ui/guide'
  import { onMount, untrack } from 'svelte'
  import { managementAuthActions } from '../api/api-client.js'
  import { HashRouter } from '../navigation/hash-router.svelte.js'
  import * as m from '../paraglide/messages.js'
  import { createManagementGuide } from '../stores/guide-store.js'
  import { ManagementStore } from '../stores/management-store.svelte.js'
  import AppShell from './AppShell.svelte'
  import VersionConflictBanner from './spacegate/VersionConflictBanner.svelte'

  interface Props {
    platform: 'web' | 'app' | 'desktop'
    apiOptions?: ManagementApiOptions
    onLocaleChange?: (locale: string) => void
  }
  const { platform, apiOptions = {}, onLocaleChange }: Props = $props()
  const pages = import.meta.glob<{ default: Component<{ store: ManagementStore }> }>('../pages/**/*.svelte')
  const router = new HashRouter(Object.keys(pages), '/overview')
  const store = new ManagementStore(router)
  // 宿主平台在组件生命周期中固定，引导控制器只创建一次。
  const guide = untrack(() => createManagementGuide(store, platform))
  let resetToken = $state('')
  const authConfig = $derived(store.authConfig && {
    ...store.authConfig,
    ui: { ...store.authConfig.ui, brandTitle: store.authConfig.ui.brandTitle || store.info?.name },
  })
  async function authenticated() {
    await store.authenticated()
    if (resetToken) {
      const url = new URL(location.href)
      url.searchParams.delete('token')
      history.replaceState(history.state, '', url)
    }
    resetToken = ''
  }
  const pageComponent = $derived(pages[router.file]())
  onMount(() => {
    const stopRouting = router.start(() => {
      if (store.user)
        void store.navigate(router.path.slice(1))
    })
    resetToken = new URL(location.href).searchParams.get('token') ?? ''
    void store.initialize(apiOptions, onLocaleChange)
    return () => {
      stopRouting()
      void store.close()
    }
  })
  $effect(() => {
    if (store.info)
      document.title = store.info.name
  })
</script>

{#key store.locale}
  {#if store.initializing}
    <div role='status' aria-live='polite' class='grid min-h-dvh place-items-center bg-base-200/50'><div class='w-72 space-y-4'><Skeleton variant='title' /><Skeleton count={3} /><p class='text-center text-sm text-base-content/50'>{m.home_loading()}</p></div></div>
  {:else if !store.user || resetToken}
    <header class='border-b border-base-content/8 bg-base-100 px-6 py-3 text-sm font-medium'>{store.info?.name}</header>
    <main data-testid='login-page'>
      {#if !store.info || !store.authConfig}
        <section class='mx-auto mt-20 max-w-md space-y-4 p-6'>
          <Alert variant='error'>{store.error || m.home_error()}</Alert>
          <Button variant='primary' onclick={() => void store.initialize(apiOptions, onLocaleChange)}>{m.home_refresh()}</Button>
        </section>
      {:else}
        <div data-testid='login-form'>
          {#if store.authConfig && store.error}<Alert variant='error' class='mx-auto max-w-lg'>{store.error}</Alert>{/if}
          <AuthPanel config={authConfig} actions={managementAuthActions} {resetToken} initialMode={resetToken ? 'reset' : 'login'} error={store.authConfig ? '' : store.error} onretry={() => void store.initialize(apiOptions, onLocaleChange)} onauthenticated={authenticated} />
        </div>
        <footer class='border-t border-base-content/8 p-3 text-center text-xs text-base-content/50'><span data-testid='app-name'>{store.info.name}</span> · <span data-testid='app-version'>v{store.info.version}</span> · <span data-testid='app-environment'>{store.info.env}</span></footer>
      {/if}
    </main>
  {:else}
    <AppShell {platform} {store} onhelp={() => guide.openHelp()}>
      <div class='flex min-h-0 flex-1 flex-col gap-5'>
        <VersionConflictBanner {store} />
        {#if store.error}<Alert variant='error' data-testid='error'>{store.error}</Alert>{/if}
        {#if store.notice}<Alert variant='success' data-testid='notice'>{store.notice}</Alert>{/if}
        {#await pageComponent}
          <div role='status' aria-label={m.home_loading()} class='space-y-4'><Skeleton variant='title' /><Skeleton count={3} /></div>
        {:then { default: Page }}
          <Page {store} />
        {/await}
      </div>
    </AppShell>
    <GuideHost {guide} locale={store.locale} scope={store.user.id} autoStart='overview' />
  {/if}
{/key}
