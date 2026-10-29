<script lang="ts">
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, Empty } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'
  import ResponsiveList from '../components/layouts/ResponsiveList.svelte'

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
    // 网关切换后旧路由不再属于当前网关，必须清空，避免用错网关/路由组合触发重载。
    store.selectedRoute = ''
    await store.loadRoutes(name)
  }
</script>

<ListPageLayout title={m.nav_instances()} description={m.instances_description()} testid="instances-page" contentClass="min-h-0 flex-1">
  {#snippet actions()}
    <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadInstances()}>{m.home_refresh()}</Button>
  {/snippet}

  <Card bordered>
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div class="flex min-w-0 flex-wrap items-center gap-2 text-sm">
        <Badge variant={online === store.instances.length && store.instances.length > 0 ? 'success' : online === 0 ? 'error' : 'warning'}>{m.instance_health_summary({ online: String(online), total: String(store.instances.length) })}</Badge>
        <span class="text-xs text-base-content/45 sm:text-sm">{m.instance_reload_hint()}</span>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Select class="w-full sm:w-48" value={store.selectedGateway} options={gatewayOptions} placeholder={m.select_gateway()} onchange={name => void selectGateway(name)} />
        <Select class="w-full sm:w-48" value={store.selectedRoute} options={routeOptions} placeholder={m.select_route()} onchange={(value) => { store.selectedRoute = value }} />
      </div>
    </div>
  </Card>

  <DataSurface class="min-h-0 flex-1">
    {#if store.instances.length === 0}
      <div class="p-6">
        <Empty title={m.instance_empty()} description={m.instances_description()} />
      </div>
    {:else}
      <ResponsiveList items={store.instances} keyOf={instance => instance}>
        {#snippet desktop()}
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
        {/snippet}
        {#snippet mobile(instance)}
          <Card bordered>
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
        {/snippet}
      </ResponsiveList>
    {/if}
  </DataSurface>
</ListPageLayout>
