<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, Empty, PageHeader } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  const gatewayOptions = $derived(store.gateways.map(name => ({ value: name, label: name })))
  const routeOptions = $derived(store.routes.map(item => ({ value: item.name, label: item.name })))
  const online = $derived(store.instances.filter(instance => store.health[instance] === true).length)

  async function reloadGateway(instance: string) {
    await store.reloadGateway(instance)
  }

  async function reloadGlobal(instance: string) {
    await store.reloadGlobal(instance)
  }

  async function reloadRoute(instance: string) {
    await store.reloadRoute(instance)
  }

  async function selectGateway(name: string) {
    store.selectedGateway = name
    await store.loadRoutes(name)
  }
</script>

<section class="space-y-5" data-testid="instances-page">
  <PageHeader title={m.nav_instances()} description={m.instances_description()}>
    {#snippet actions()}
      <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadInstances()}>{m.home_refresh()}</Button>
    {/snippet}
  </PageHeader>

  <Card bordered shadow={false}>
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="flex flex-wrap items-center gap-2 text-sm">
        <Badge variant={online === store.instances.length && store.instances.length > 0 ? 'success' : online === 0 ? 'error' : 'warning'}>{m.instance_health_summary({ online: String(online), total: String(store.instances.length) })}</Badge>
        <span class="text-base-content/45">{m.instance_reload_hint()}</span>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Select class="w-48" value={store.selectedGateway} options={gatewayOptions} placeholder={m.select_gateway()} onchange={name => void selectGateway(name)} />
        <Select class="w-48" value={store.selectedRoute} options={routeOptions} placeholder={m.select_route()} onchange={(value) => { store.selectedRoute = value }} />
      </div>
    </div>
  </Card>

  {#if store.instances.length === 0}
    <div class="rounded-xl border border-base-content/8 bg-base-100 p-6">
      <Empty title={m.instance_empty()} description={m.instances_description()} />
    </div>
  {:else}
    <div class="hidden overflow-auto rounded-xl border border-base-content/8 bg-base-100 md:block">
      <table class="table w-full text-sm">
        <thead>
          <tr class="text-left text-xs text-base-content/50">
            <th class="px-3 py-2">{m.instance_id()}</th>
            <th class="px-3 py-2">{m.instance_status()}</th>
            <th class="px-3 py-2">{m.actions()}</th>
          </tr>
        </thead>
        <tbody>
          {#each store.instances as instance (instance)}
            <tr class="border-t border-base-content/8">
              <td class="break-all px-3 py-2 font-mono text-xs">{instance}</td>
              <td class="px-3 py-2">
                <Badge size="sm" variant={store.health[instance] === true ? 'success' : store.health[instance] === false ? 'error' : 'ghost'}>
                  {store.health[instance] === true ? m.instance_online() : store.health[instance] === false ? m.instance_offline() : m.instance_unknown()}
                </Badge>
              </td>
              <td class="px-3 py-2">
                <div class="flex flex-wrap gap-1">
                  <Button size="xs" variant="outline" loading={store.saving} disabled={!store.selectedGateway} onclick={() => void reloadGateway(instance)}>{m.instance_reload_gateway()}</Button>
                  <Button size="xs" variant="ghost" loading={store.saving} onclick={() => void reloadRoute(instance)} disabled={!store.selectedGateway || !store.selectedRoute}>{m.instance_reload_route()}</Button>
                  <Button size="xs" variant="error" loading={store.saving} onclick={() => void reloadGlobal(instance)}>{m.instance_reload_global()}</Button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="space-y-3 md:hidden">
      {#each store.instances as instance (instance)}
        <Card bordered shadow={false}>
          <div class="flex items-start justify-between gap-2">
            <span class="min-w-0 break-all font-mono text-xs">{instance}</span>
            <Badge size="sm" variant={store.health[instance] === true ? 'success' : store.health[instance] === false ? 'error' : 'ghost'}>
              {store.health[instance] === true ? m.instance_online() : store.health[instance] === false ? m.instance_offline() : m.instance_unknown()}
            </Badge>
          </div>
          <div class="mt-3 flex flex-wrap gap-1">
            <Button size="xs" variant="outline" loading={store.saving} disabled={!store.selectedGateway} onclick={() => void reloadGateway(instance)}>{m.instance_reload_gateway()}</Button>
            <Button size="xs" variant="ghost" loading={store.saving} onclick={() => void reloadRoute(instance)} disabled={!store.selectedGateway || !store.selectedRoute}>{m.instance_reload_route()}</Button>
            <Button size="xs" variant="error" loading={store.saving} onclick={() => void reloadGlobal(instance)}>{m.instance_reload_global()}</Button>
          </div>
        </Card>
      {/each}
    </div>
  {/if}
</section>
