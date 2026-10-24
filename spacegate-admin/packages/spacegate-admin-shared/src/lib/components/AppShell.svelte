<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { ManagementPage, ManagementStore } from '../stores/management-store.svelte.js'
  import { Avatar, Badge, BareButton, Button } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'

  interface Props {
    platform: 'web' | 'app' | 'desktop' | 'miniapp'
    store: ManagementStore
    children: Snippet
    onhelp: () => void
  }
  const { platform, store, children, onhelp }: Props = $props()
  let menuOpen = $state(false)
  const items = $derived([
    ...(store.isAdmin
? [
      { id: 'overview' as const, label: m.nav_overview(), icon: 'icon-[tabler--layout-dashboard]' },
      { id: 'gateways' as const, label: m.nav_gateways(), icon: 'icon-[tabler--network]' },
      { id: 'routes' as const, label: m.nav_routes(), icon: 'icon-[tabler--route]' },
      { id: 'plugins' as const, label: m.nav_plugins(), icon: 'icon-[tabler--plug]' },
      { id: 'ai-queue' as const, label: m.nav_ai_queue(), icon: 'icon-[tabler--clock-hour-4]' },
      { id: 'instances' as const, label: m.nav_instances(), icon: 'icon-[tabler--server-2]' },
      { id: 'users' as const, label: m.nav_users(), icon: 'icon-[tabler--users]' },
      { id: 'roles' as const, label: m.nav_roles(), icon: 'icon-[tabler--shield-lock]' },
      { id: 'audit' as const, label: m.nav_audit(), icon: 'icon-[tabler--file-text]' },
    ]
: []),
    { id: 'settings' as const, label: m.settings_title(), icon: 'icon-[tabler--settings]' },
  ])
  const title = $derived(items.find(item => item.id === store.page)?.label ?? store.info?.name ?? '')

  function navigate(page: ManagementPage) {
    menuOpen = false
    void store.navigate(page)
  }
</script>

<div class="flex h-dvh overflow-hidden bg-base-200/50 text-base-content" class:hai-safe-area={platform === 'app'}>
  {#if menuOpen}
    <BareButton class="fixed inset-0 z-30 bg-black/30 lg:hidden" ariaLabel={m.close_menu()} onclick={() => { menuOpen = false }} />
  {/if}
  <aside class="fixed inset-y-0 left-0 z-40 flex w-60 shrink-0 flex-col border-r border-base-content/8 bg-base-100 transition-transform duration-200 lg:sticky lg:top-0 lg:h-dvh lg:translate-x-0 {menuOpen ? 'translate-x-0' : '-translate-x-full'}">
    <div class="flex h-16 items-center gap-3 border-b border-base-content/8 px-5">
      <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary text-primary-content"><span class="icon-[tabler--hexagon-letter-h] size-5"></span></span>
      <span class="truncate text-sm font-semibold" data-testid="app-name">{store.info?.name}</span>
    </div>
    <nav class="flex-1 space-y-1 overflow-y-auto p-3" aria-label={m.navigation()}>
      {#each items as item (item.id)}
        <BareButton class="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors {store.page === item.id ? 'bg-primary/10 font-medium text-primary' : 'text-base-content/60 hover:bg-base-content/4'}" onclick={() => navigate(item.id)} data-testid="nav-{item.id}">
          <span class="{item.icon} size-4.5"></span>{item.label}
        </BareButton>
      {/each}
    </nav>
    <div class="border-t border-base-content/8 p-4 text-xs text-base-content/40"><span data-testid="app-version">v{store.info?.version}</span></div>
  </aside>
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    <header class="sticky top-0 z-20 flex h-16 shrink-0 items-center justify-between gap-2 border-b border-base-content/8 bg-base-100/95 px-3 backdrop-blur sm:px-4 lg:px-6">
      <div class="flex min-w-0 flex-1 items-center gap-2 sm:gap-3">
        <BareButton class="rounded-lg p-2 hover:bg-base-content/5 lg:hidden" ariaLabel={m.open_menu()} onclick={() => { menuOpen = !menuOpen }} data-testid="mobile-menu"><span class="icon-[tabler--menu-2] size-5"></span></BareButton>
        <span class="truncate text-sm font-medium">{title}</span>
      </div>
      <div class="flex shrink-0 items-center gap-1.5 sm:gap-3">
        <Button variant="ghost" size="sm" onclick={onhelp} data-guide="admin-help" data-testid="admin-help" ariaLabel={m.guide_help_title()}><span class="icon-[tabler--help-circle] size-4"></span><span class="hidden sm:inline">{m.guide_help_title()}</span></Button>
        <Badge variant="ghost" size="sm" data-testid="app-environment">{store.info?.env}</Badge>
        {#if store.user}
          <BareButton class="flex items-center gap-2 rounded-lg px-1 py-1 hover:bg-base-content/5 sm:px-2" ariaLabel={m.settings_title()} onclick={() => navigate('settings')} data-guide="admin-profile">
            <Avatar name={store.user.displayName || store.user.username} src={store.user.avatarUrl} size="sm" />
            <span class="hidden max-w-36 truncate text-sm sm:block">{store.user.displayName || store.user.username}</span>
          </BareButton>
        {/if}
        <Button variant="ghost" size="sm" loading={store.saving} onclick={() => void store.logout()} data-testid="logout">{m.logout()}</Button>
      </div>
    </header>
    <main class="flex min-h-0 w-full min-w-0 flex-1 flex-col overflow-auto p-4 pb-8 sm:p-6 lg:p-8" data-guide="admin-content">{@render children()}</main>
    <footer class="shrink-0 border-t border-base-content/8 px-6 py-3 text-center text-xs text-base-content/35">{store.info?.name} · v{store.info?.version}</footer>
  </div>
</div>
