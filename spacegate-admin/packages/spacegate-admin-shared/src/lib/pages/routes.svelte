<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import type { JsonRecord } from '../spacegate/model.js'
  import { Card, DataTable, Empty, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Input, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { asRecord, asRecordArray, backendSummary, readNumber, routeMatchSummary, routeTypeOf } from '../spacegate/model.js'
  import RouteEditor from '../components/spacegate/RouteEditor.svelte'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'
  import ResponsiveList from '../components/layouts/ResponsiveList.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  let editorOpen = $state(false)
  let editorName = $state<string | null>(null)
  let search = $state('')

  const gatewayOptions = $derived(store.gateways.map(name => ({ value: name, label: name })))
  const rows = $derived(store.routes.filter((row) => {
    const term = search.trim().toLowerCase()
    if (!term)
      return true
    return row.name.toLowerCase().includes(term) || routeMatchSummary(row.route).toLowerCase().includes(term)
  }))
  const columns = $derived([
    { key: 'name', label: m.route_name() },
    { key: 'type', label: m.route_type() },
    { key: 'match', label: m.route_match() },
    { key: 'backend', label: m.route_backends() },
    { key: 'priority', label: m.route_priority() },
    { key: 'plugins', label: m.route_plugins() },
  ])

  function routeOf(row: { route: unknown }): JsonRecord {
    return asRecord(row.route) ?? {}
  }

  function firstBackend(route: JsonRecord): string {
    if (route.kind === 'MCPRoute')
      return backendSummary(asRecordArray(route.backends)[0]) || '—'
    const rule = asRecordArray(route.rules)[0]
    return backendSummary(asRecordArray(rule?.backends)[0]) || '—'
  }

  function openCreate() {
    editorName = null
    editorOpen = true
  }

  function openEdit(name: string) {
    store.selectedRoute = name
    editorName = name
    editorOpen = true
  }
</script>

<ListPageLayout title={m.nav_routes()} description={m.routes_description()} testid="routes-page" contentClass="min-h-0 flex-1">
  {#snippet actions()}
    <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadRoutes()}>{m.home_refresh()}</Button>
    <Button variant="primary" size="sm" disabled={!store.selectedGateway} onclick={openCreate} data-testid="route-create">{m.route_create()}</Button>
  {/snippet}

  <DataSurface class="min-h-0 flex-1">
    {#snippet toolbar()}
      <div class="w-full sm:w-56">
        <Select value={store.selectedGateway} options={gatewayOptions} placeholder={m.select_gateway()} onchange={name => void store.selectGateway(name)} />
      </div>
      <div class="min-w-0 flex-1 sm:min-w-48">
        <Input type="search" bind:value={search} placeholder={m.routes_search()} />
      </div>
    {/snippet}
    {#if store.loading && store.routes.length === 0}
      <div class="p-4"><Skeleton count={4} /></div>
    {:else}
      <ResponsiveList items={rows} keyOf={row => row.name}>
        {#snippet desktop()}
          <DataTable data={rows} {columns} keyField="name" loading={store.loading} striped={false} density="compact">
            {#snippet cell(row, key)}
              {@const route = routeOf(row)}
              {#if key === 'name'}
                <span class="font-medium">{row.name}</span>
              {:else if key === 'type'}
                <Badge size="sm" variant="ghost">{routeTypeOf(route) === 'mcp' ? m.route_type_mcp() : m.route_type_http()}</Badge>
              {:else if key === 'match'}
                <span class="font-mono text-xs">{routeMatchSummary(route) || '—'}</span>
              {:else if key === 'backend'}
                <span class="font-mono text-xs">{firstBackend(route)}</span>
              {:else if key === 'priority'}
                <span>{route.kind === 'MCPRoute' ? '—' : String(readNumber(route, 'priority') ?? 0)}</span>
              {:else if key === 'plugins'}
                <Badge size="sm" variant="ghost">{asRecordArray(route.plugins).length}</Badge>
              {/if}
            {/snippet}
            {#snippet actions(row)}
              <Button size="xs" variant="ghost" onclick={() => openEdit(row.name)}>{m.edit()}</Button>
            {/snippet}
          </DataTable>
        {/snippet}
        {#snippet mobile(row)}
          {@const route = routeOf(row)}
          <Card bordered>
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <p class="break-all font-medium">{row.name}</p>
                <p class="mt-1 font-mono text-xs text-base-content/50">{routeMatchSummary(route) || '—'}</p>
              </div>
              <Badge size="sm" variant="ghost">{routeTypeOf(route) === 'mcp' ? m.route_type_mcp() : m.route_type_http()}</Badge>
            </div>
            <p class="mt-2 flex items-center gap-2 text-xs text-base-content/45">
              <span class="icon-[tabler--arrow-up-right] size-3.5 shrink-0"></span>
              <span class="truncate font-mono">{firstBackend(route)}</span>
            </p>
            <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
              <Badge size="sm" variant="ghost">{asRecordArray(route.plugins).length} {m.route_plugins()}</Badge>
              <Button size="xs" variant="outline" onclick={() => openEdit(row.name)}>{m.edit()}</Button>
            </div>
          </Card>
        {/snippet}
        {#snippet empty()}
          <Empty title={m.route_empty()} description={m.routes_description()}>
            {#snippet action()}
              <Button size="sm" variant="primary" disabled={!store.selectedGateway} onclick={openCreate}>{m.route_create()}</Button>
            {/snippet}
          </Empty>
        {/snippet}
      </ResponsiveList>
    {/if}
  </DataSurface>
</ListPageLayout>

<RouteEditor bind:open={editorOpen} routeName={editorName} {store} />
