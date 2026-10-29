<script lang="ts">
  import type { AdminRole } from 'spacegate-admin-contract'
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, Modal, PageHeader } from '@h-ai/ui/components/compounds/index'
  import { Badge, Button, Textarea } from '@h-ai/ui/components/primitives/index'
  import * as m from '../paraglide/messages.js'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()
  let editing = $state<AdminRole | null>(null)
  let description = $state('')
  function edit(role: AdminRole) {
    editing = role
    description = role.description ?? ''
    store.error = ''
  }
  async function save(event: SubmitEvent) {
    event.preventDefault()
    if (editing && await store.saveRole(editing.id, description))
      editing = null
  }
</script>

<section class="space-y-5" data-testid="roles-page">
  <PageHeader title={m.nav_roles()} description={m.roles_description()}>
    {#snippet actions()}
      <Button variant="outline" size="sm" loading={store.loading} onclick={() => void store.loadRoles()}>{m.home_refresh()}</Button>
    {/snippet}
  </PageHeader>

  <div class="grid gap-5 md:grid-cols-2">
    {#each store.roles as role (role.id)}
      <Card bordered>
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="grid size-9 place-items-center rounded-lg bg-primary/10 text-primary" aria-hidden="true"><span class="{role.code === 'admin' ? 'icon-[tabler--shield-lock]' : 'icon-[tabler--user]'} size-4.5"></span></span>
            <h2 class="text-lg font-semibold">{role.code === 'admin' ? m.role_admin() : m.role_user()}</h2>
          </div>
          <Badge variant={role.code === 'admin' ? 'primary' : 'ghost'}>{role.code}</Badge>
        </div>
        <p class="mt-3 text-sm leading-6 text-base-content/60">{role.code === 'admin' ? m.role_admin_capabilities() : m.role_user_capabilities()}</p>
        <p class="mt-4 text-xs text-base-content/45">{m.role_members({ count: String(role.userCount) })}</p>
        {#if role.description}<p class="mt-3 text-sm text-base-content/70">{role.description}</p>{/if}
        <div class="mt-5"><Button variant="outline" size="sm" onclick={() => edit(role)}>{m.role_edit()}</Button></div>
      </Card>
    {/each}
  </div>
</section>

<Modal open={!!editing} title={m.role_edit()} onclose={() => { editing = null }} closeOnBackdrop={!store.saving} closeOnEscape={!store.saving}>
  <form class="space-y-4" onsubmit={save}>
    <label class="block text-sm font-medium" for="role-description">{m.role_description()}</label>
    <Textarea id="role-description" bind:value={description} rows={4} data-testid="role-description" />
    {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}
    <div class="flex justify-end gap-2"><Button variant="ghost" disabled={store.saving} onclick={() => { editing = null }}>{m.cancel()}</Button><Button type="submit" variant="primary" loading={store.saving}>{m.save()}</Button></div>
  </form>
</Modal>
