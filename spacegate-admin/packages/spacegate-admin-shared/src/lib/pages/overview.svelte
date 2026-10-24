<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, Empty, PageHeader } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import { asRecord, asRecordArray, readNumber, readString, routeMatchSummary } from '../spacegate/model.js'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  const online = $derived(store.instances.filter(instance => store.health[instance] === true).length)
  const gateway = $derived(asRecord(store.gateway))
  const listeners = $derived(asRecordArray(gateway?.listeners))
  const parameters = $derived(asRecord(gateway?.parameters))
  const routePreview = $derived(store.routes.slice(0, 6))
</script>

<section class="space-y-5" data-testid="overview-page">
  <PageHeader title={m.nav_overview()} description={m.overview_description()}>
    {#snippet actions()}
      <div class="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadOverview()}>{m.home_refresh()}</Button>
        <Button variant="primary" size="sm" onclick={() => void store.navigate('gateways')}>{m.gateway_create()}</Button>
      </div>
    {/snippet}
  </PageHeader>

  <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
    <Card bordered shadow={false}>
      <p class="text-xs text-base-content/50">{m.overview_gateway_count()}</p>
      <p class="mt-2 text-2xl font-semibold" data-testid="overview-gateways">{store.gateways.length}</p>
      <Button class="mt-3" size="xs" variant="ghost" onclick={() => void store.navigate('gateways')}>{m.overview_open()}</Button>
    </Card>
    <Card bordered shadow={false}>
      <p class="text-xs text-base-content/50">{m.overview_route_count()}</p>
      <p class="mt-2 text-2xl font-semibold" data-testid="overview-routes">{store.routes.length}</p>
      <Button class="mt-3" size="xs" variant="ghost" onclick={() => void store.navigate('routes')}>{m.overview_open()}</Button>
    </Card>
    <Card bordered shadow={false}>
      <p class="text-xs text-base-content/50">{m.overview_plugin_count()}</p>
      <p class="mt-2 text-2xl font-semibold" data-testid="overview-plugins">{store.plugins.length}</p>
      <Button class="mt-3" size="xs" variant="ghost" onclick={() => void store.navigate('plugins')}>{m.overview_open()}</Button>
    </Card>
    <Card bordered shadow={false}>
      <p class="text-xs text-base-content/50">{m.overview_instance_health()}</p>
      <p class="mt-2 text-2xl font-semibold" data-testid="overview-health">{online}/{store.instances.length}</p>
      <Button class="mt-3" size="xs" variant="ghost" onclick={() => void store.navigate('instances')}>{m.overview_open()}</Button>
    </Card>
  </div>

  <div class="grid gap-5 xl:grid-cols-2">
    <Card title={m.overview_current_gateway()} bordered shadow={false}>
      {#if gateway}
        <div class="space-y-3">
          <div class="flex flex-wrap items-center gap-2">
            <span class="font-medium">{readString(gateway, 'name')}</span>
            <Badge size="sm" variant="ghost">{m.version()} {store.version}</Badge>
          </div>
          <p class="text-sm text-base-content/60">{m.gateway_listener_summary({ listeners: listeners.map(item => `${readString(item, 'name')}:${readNumber(item, 'port') ?? ''}`).join(', ') || '—' })}</p>
          <p class="text-sm text-base-content/60">{m.gateway_plugin_count({ count: String(asRecordArray(gateway?.plugins).length) })}</p>
          <p class="break-all text-xs text-base-content/45">{m.gateway_redis_url()}: {readString(parameters, 'redis_url') || '—'}</p>
          <div class="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onclick={() => void store.navigate('gateways')}>{m.gateway_edit()}</Button>
            <Button size="sm" variant="ghost" onclick={() => void store.navigate('routes')}>{m.overview_configure_routes()}</Button>
          </div>
        </div>
      {:else}
        <Empty title={m.overview_no_gateway()} description={m.gateway_empty()}>
          {#snippet action()}
            <Button size="sm" variant="primary" onclick={() => void store.navigate('gateways')}>{m.gateway_create()}</Button>
          {/snippet}
        </Empty>
      {/if}
    </Card>

    <Card title={m.overview_instance_health()} bordered shadow={false}>
      {#if store.instances.length === 0}
        <Empty title={m.instance_empty()} description={m.overview_health_empty()} />
      {:else}
        <ul class="divide-y divide-base-content/8">
          {#each store.instances as instance (instance)}
            <li class="flex items-center justify-between gap-3 py-2.5 text-sm">
              <span class="min-w-0 break-all font-mono text-xs">{instance}</span>
              <Badge size="sm" variant={store.health[instance] === true ? 'success' : store.health[instance] === false ? 'error' : 'ghost'}>
                {store.health[instance] === true ? m.instance_online() : store.health[instance] === false ? m.instance_offline() : m.instance_unknown()}
              </Badge>
            </li>
          {/each}
        </ul>
        <Button class="mt-3" size="sm" variant="ghost" onclick={() => void store.navigate('instances')}>{m.instance_reload_global()}</Button>
      {/if}
    </Card>
  </div>

  <Card title={m.overview_routes()} bordered shadow={false}>
    {#if routePreview.length === 0}
      <Empty title={m.route_empty()} description={m.routes_description()} />
    {:else}
      <div class="space-y-2">
        {#each routePreview as item (item.name)}
          {@const record = asRecord(item.route) ?? {}}
          <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-base-content/8 px-3 py-2 text-sm">
            <span class="min-w-0 break-all font-medium">{item.name}</span>
            <div class="flex flex-wrap items-center gap-2 text-xs text-base-content/50">
              <Badge size="sm" variant="ghost">{record.kind === 'MCPRoute' ? m.route_type_mcp() : m.route_type_http()}</Badge>
              <span class="font-mono">{routeMatchSummary(record) || readString(record, 'route_name')}</span>
            </div>
          </div>
        {/each}
        <Button size="sm" variant="outline" onclick={() => void store.navigate('routes')}>{m.overview_view_all_routes()}</Button>
      </div>
    {/if}
  </Card>
</section>
