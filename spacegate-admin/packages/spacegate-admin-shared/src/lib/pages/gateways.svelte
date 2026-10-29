<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, DataTable, Empty, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { asRecord, readNumber, readString } from '../spacegate/model.js'
  import GatewayEditor from '../components/spacegate/GatewayEditor.svelte'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'
  import ResponsiveList from '../components/layouts/ResponsiveList.svelte'

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

<ListPageLayout title={m.nav_gateways()} description={m.gateways_description()} testid="gateways-page" contentClass="min-h-0 flex-1">
  {#snippet actions()}
    <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadGateways()}>{m.home_refresh()}</Button>
    <Button variant="primary" size="sm" onclick={openCreate} data-testid="gateway-create">{m.gateway_create()}</Button>
  {/snippet}

  <DataSurface class="min-h-0 flex-1">
    {#snippet toolbar()}
      <div class="w-full sm:max-w-72">
        <Input type="search" bind:value={search} placeholder={m.gateways_search()} />
      </div>
    {/snippet}
    {#if store.loading && store.gatewaySummaries.length === 0}
      <div class="p-4"><Skeleton count={4} /></div>
    {:else}
      <ResponsiveList items={rows} keyOf={row => row.name}>
        {#snippet desktop()}
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
        {/snippet}
        {#snippet mobile(row)}
          <Card bordered>
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <p class="break-all font-medium">{row.name}</p>
                <p class="mt-1 font-mono text-xs text-base-content/50">{listenerSummary(row.gateway)}</p>
              </div>
              <Badge size="sm" variant="ghost">{row.routeCount} {m.route_plural()}</Badge>
            </div>
            <p class="mt-2 flex items-center gap-2 text-xs text-base-content/45">
              <span class="icon-[tabler--database] size-3.5 shrink-0"></span>
              <span class="truncate font-mono">{readString(asRecord(row.gateway?.parameters), 'redis_url') || '—'}</span>
            </p>
            <div class="mt-3 flex flex-wrap gap-1">
              <Button size="xs" variant="outline" onclick={() => void openRoutes(row.name)}>{m.route_plural()}</Button>
              <Button size="xs" variant="ghost" onclick={() => openEdit(row.name)}>{m.edit()}</Button>
            </div>
          </Card>
        {/snippet}
        {#snippet empty()}
          <Empty title={m.gateway_empty()} description={m.gateways_description()}>
            {#snippet action()}<Button size="sm" variant="primary" onclick={openCreate}>{m.gateway_create()}</Button>{/snippet}
          </Empty>
        {/snippet}
      </ResponsiveList>
    {/if}
  </DataSurface>
</ListPageLayout>

<GatewayEditor bind:open={editorOpen} gatewayName={editorName} {store} />
