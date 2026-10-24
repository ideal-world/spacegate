<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, DataTable, Empty, PageHeader, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { asRecord, readNumber, readString } from '../spacegate/model.js'
  import GatewayEditor from '../components/spacegate/GatewayEditor.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  let editorOpen = $state(false)
  let editorName = $state<string | null>(null)
  let search = $state('')

  const rows = $derived(store.gatewaySummaries.filter((row) => {
    const term = search.trim().toLowerCase()
    return !term || row.name.toLowerCase().includes(term)
  }))
  const columns = $derived([
    { key: 'name', label: m.gateway_name() },
    { key: 'listeners', label: m.gateway_listeners() },
    { key: 'routes', label: m.gateway_route_count() },
    { key: 'plugins', label: m.gateway_plugins() },
    { key: 'redis', label: m.gateway_redis_url() },
  ])

  function listenerSummary(gateway: Record<string, unknown> | null): string {
    const listeners = Array.isArray(gateway?.listeners) ? gateway.listeners : []
    return listeners.map((item) => {
      const record = item as Record<string, unknown>
      return `${readString(record, 'name')}:${readNumber(record, 'port') ?? ''}`
    }).join(', ') || '—'
  }

  function openCreate() {
    editorName = null
    editorOpen = true
  }

  function openEdit(name: string) {
    editorName = name
    editorOpen = true
  }

  async function openRoutes(name: string) {
    store.selectedGateway = name
    await store.navigate('routes')
  }
</script>

<section class="space-y-5" data-testid="gateways-page">
  <PageHeader title={m.nav_gateways()} description={m.gateways_description()}>
    {#snippet actions()}
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadGateways()}>{m.home_refresh()}</Button>
        <Button variant="primary" size="sm" onclick={openCreate} data-testid="gateway-create">{m.gateway_create()}</Button>
      </div>
    {/snippet}
  </PageHeader>

  <div class="rounded-xl border border-base-content/8 bg-base-100">
    <div class="border-b border-base-content/8 p-4">
      <Input type="search" bind:value={search} placeholder={m.gateways_search()} />
    </div>
    {#if store.loading && store.gatewaySummaries.length === 0}
      <div class="p-4"><Skeleton count={4} /></div>
    {:else if rows.length === 0}
      <div class="p-6">
        <Empty title={m.gateway_empty()} description={m.gateways_description()}>
          {#snippet action()}<Button size="sm" variant="primary" onclick={openCreate}>{m.gateway_create()}</Button>{/snippet}
        </Empty>
      </div>
    {:else}
      <div class="hidden overflow-auto md:block">
        <DataTable data={rows} {columns} keyField="name" loading={store.loading} striped={false} density="compact">
          {#snippet cell(row, key)}
            {#if key === 'name'}
              <span class="font-medium">{row.name}</span>
            {:else if key === 'listeners'}
              <span class="font-mono text-xs">{listenerSummary(row.gateway)}</span>
            {:else if key === 'routes'}
              <Badge size="sm" variant="ghost">{row.routeCount}</Badge>
            {:else if key === 'plugins'}
              <Badge size="sm" variant="ghost">{row.pluginCount}</Badge>
            {:else if key === 'redis'}
              <span class="block max-w-56 truncate font-mono text-xs">{readString(asRecord(row.gateway?.parameters), 'redis_url') || '—'}</span>
            {/if}
          {/snippet}
          {#snippet actions(row)}
            <div class="flex flex-wrap items-center gap-1">
              <Button size="xs" variant="ghost" onclick={() => void openRoutes(row.name)}>{m.route_plural()}</Button>
              <Button size="xs" variant="ghost" onclick={() => openEdit(row.name)}>{m.edit()}</Button>
            </div>
          {/snippet}
        </DataTable>
      </div>
      <div class="space-y-3 p-4 md:hidden">
        {#each rows as row (row.name)}
          <Card bordered shadow={false}>
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <p class="break-all font-medium">{row.name}</p>
                <p class="mt-1 font-mono text-xs text-base-content/50">{listenerSummary(row.gateway)}</p>
              </div>
              <Badge size="sm" variant="ghost">{row.routeCount} {m.route_plural()}</Badge>
            </div>
            <p class="mt-2 truncate text-xs text-base-content/45">{readString(asRecord(row.gateway?.parameters), 'redis_url') || '—'}</p>
            <div class="mt-3 flex flex-wrap gap-1">
              <Button size="xs" variant="outline" onclick={() => void openRoutes(row.name)}>{m.route_plural()}</Button>
              <Button size="xs" variant="ghost" onclick={() => openEdit(row.name)}>{m.edit()}</Button>
            </div>
          </Card>
        {/each}
      </div>
    {/if}
  </div>
</section>

<GatewayEditor bind:open={editorOpen} gatewayName={editorName} {store} />
