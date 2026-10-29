<script lang="ts">
  import type { AdminAuditLog } from 'spacegate-admin-contract'
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { DataTable, Modal, Pagination } from '@h-ai/ui/components/compounds/index'
  import { Button, Input } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()
  let selected = $state<AdminAuditLog | null>(null)
  const columns = $derived([
    { key: 'createdAt', label: m.created_at() },
    { key: 'username', label: m.audit_actor() },
    { key: 'action', label: m.audit_action() },
    { key: 'resource', label: m.audit_resource() },
    { key: 'ipAddress', label: m.audit_ip() },
  ])
</script>

<ListPageLayout title={m.nav_audit()} description={m.audit_description()} testid="audit-page" contentClass="min-h-0 flex-1">
  <DataSurface class="min-h-0 flex-1">
    {#snippet toolbar()}
      <form class="grid w-full items-end gap-3 sm:grid-cols-2 xl:grid-cols-[repeat(4,minmax(0,1fr))_auto] [&_.fieldset]:p-0" onsubmit={(event) => {
        event.preventDefault()
        void store.loadAudit()
      }}>
        <div class="min-w-0 space-y-1"><label class="block text-xs text-base-content/60" for="audit-action">{m.audit_action()}</label><Input id="audit-action" bind:value={store.auditAction} placeholder={m.audit_action_placeholder()} data-testid="audit-action" /></div>
        <div class="min-w-0 space-y-1"><label class="block text-xs text-base-content/60" for="audit-resource">{m.audit_resource()}</label><Input id="audit-resource" bind:value={store.auditResource} placeholder={m.audit_resource_placeholder()} data-testid="audit-resource" /></div>
        <div class="min-w-0 space-y-1"><label class="block text-xs text-base-content/60" for="audit-start">{m.audit_start()}</label><input id="audit-start" type="date" class="input block h-10 min-w-0 w-full" bind:value={store.auditStartDate} /></div>
        <div class="min-w-0 space-y-1"><label class="block text-xs text-base-content/60" for="audit-end">{m.audit_end()}</label><input id="audit-end" type="date" class="input block h-10 min-w-0 w-full" bind:value={store.auditEndDate} /></div>
        <Button type="submit" variant="outline" disabled={store.loading} class="h-10 w-full">{m.search()}</Button>
      </form>
    {/snippet}

    <div class="flex min-h-0 flex-1 flex-col overflow-auto">
      <DataTable data={store.audit} {columns} keyField="id" loading={store.loading} striped={false} class="min-h-0 flex-1 overflow-auto">
        {#snippet cell(entry, key)}
          {#if key === 'createdAt'}<span class="whitespace-nowrap text-xs">{store.formatDate(entry.createdAt)}</span>
          {:else if key === 'username'}{entry.username || m.audit_system()}
          {:else if key === 'action'}<span class="rounded-md bg-base-200 px-2 py-1 font-mono text-xs">{entry.action}</span>
          {:else if key === 'resource'}<span class="text-sm">{entry.resource}</span>
          {:else if key === 'ipAddress'}<span class="text-xs text-base-content/55">{entry.ipAddress || '—'}</span>{/if}
        {/snippet}
        {#snippet actions(entry)}<Button size="xs" variant="ghost" onclick={() => { selected = entry }}>{m.details()}</Button>{/snippet}
      </DataTable>
    </div>

    {#snippet footer()}
      <Pagination page={store.auditPage} total={store.auditTotal} pageSize={store.pageSize} onchange={page => void store.loadAudit(page)} />
    {/snippet}
  </DataSurface>
</ListPageLayout>

<Modal open={!!selected} title={m.audit_details()} onclose={() => { selected = null }}>
  {#if selected}
    <dl class="grid grid-cols-[auto_1fr] gap-x-5 gap-y-3 text-sm">
      <dt class="text-base-content/50">{m.created_at()}</dt><dd>{store.formatDate(selected.createdAt)}</dd>
      <dt class="text-base-content/50">{m.audit_actor()}</dt><dd class="break-all">{selected.username || m.audit_system()}</dd>
      <dt class="text-base-content/50">{m.audit_action()}</dt><dd>{selected.action}</dd>
      <dt class="text-base-content/50">{m.audit_resource()}</dt><dd>{selected.resource}</dd>
      <dt class="text-base-content/50">{m.audit_resource_id()}</dt><dd class="break-all">{selected.resourceId || '—'}</dd>
      <dt class="text-base-content/50">{m.audit_ip()}</dt><dd>{selected.ipAddress || '—'}</dd>
    </dl>
    <pre class="mt-5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-base-200 p-4 text-xs">{selected.details || '—'}</pre>
  {/if}
</Modal>
