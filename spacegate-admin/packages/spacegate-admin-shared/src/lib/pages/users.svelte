<script lang="ts">
  import type { AdminRoleCode, AdminUser } from 'spacegate-admin-contract'
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, DataTable, FormField, FormLayout, Modal, Pagination, Skeleton } from '@h-ai/ui/components/compounds/index'
  import { Avatar, Badge, BareInput, Button, Checkbox, Input, Select } from '@h-ai/ui/components/primitives/index'
  import { PasswordInput } from '@h-ai/ui/components/scenes/iam/index'
  import * as m from '../paraglide/messages.js'
  import DataSurface from '../components/layouts/DataSurface.svelte'
  import ListPageLayout from '../components/layouts/ListPageLayout.svelte'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()

  let editorOpen = $state(false)
  let editing = $state<AdminUser | null>(null)
  let deleting = $state<AdminUser | null>(null)
  let username = $state('')
  let displayName = $state('')
  let email = $state('')
  let password = $state('')
  let role = $state<AdminRoleCode>('user')
  let enabled = $state(true)
  const columns = $derived([
    { key: 'username', label: m.user_account() },
    { key: 'email', label: m.user_email() },
    { key: 'roles', label: m.user_role() },
    { key: 'enabled', label: m.user_status() },
    { key: 'createdAt', label: m.created_at() },
  ])
  const roleOptions = $derived([{ value: 'admin' as const, label: m.role_admin() }, { value: 'user' as const, label: m.role_user() }])
  const statusOptions = $derived([{ value: 'all', label: m.all_statuses() }, { value: 'enabled', label: m.enabled() }, { value: 'disabled', label: m.disabled() }])
  // 初始密码下限跟随服务端策略，避免按钮先放行、提交时被服务端拒绝。
  const minPasswordLength = $derived(store.authConfig?.password.minLength ?? 8)
  const canSubmit = $derived(!!editing || password.length >= minPasswordLength)

  function openEditor(user: AdminUser | null) {
    editing = user
    username = user?.username ?? ''
    displayName = user?.displayName ?? ''
    email = user?.email ?? ''
    password = ''
    role = user?.roles.includes('admin') ? 'admin' : 'user'
    enabled = user?.enabled ?? true
    store.error = ''
    editorOpen = true
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    const fields = { displayName: displayName.trim(), role, enabled }
    const saved = await store.saveUser(editing ? { id: editing.id, email: email.trim(), ...fields } : { username: username.trim(), password, email: email.trim() || undefined, ...fields })
    if (saved) {
      editorOpen = false
      password = ''
    }
  }

  async function confirmDelete() {
    if (deleting && await store.deleteUser(deleting))
      deleting = null
  }

  function upload(user: AdminUser, event: Event & { currentTarget: HTMLInputElement }) {
    const file = event.currentTarget.files?.[0]
    if (file)
      void store.uploadAvatar(file, user.id)
    event.currentTarget.value = ''
  }
</script>

<ListPageLayout title={m.nav_users()} description={m.users_description()} testid="users-page" contentClass="min-h-0 flex-1">
  {#snippet actions()}
    <Button variant="primary" onclick={() => openEditor(null)} data-testid="create-user">{m.user_create()}</Button>
  {/snippet}

  <DataSurface class="min-h-0 flex-1">
    {#snippet toolbar()}
      <form class="flex w-full flex-wrap items-center gap-3" onsubmit={(event) => {
        event.preventDefault()
        void store.loadUsers()
      }}>
        <div class="min-w-0 flex-1 sm:min-w-52">
          <Input type="search" bind:value={store.usersSearch} placeholder={m.users_search()} data-testid="users-search" />
        </div>
        <div class="w-full sm:w-40"><Select bind:value={store.usersStatus} options={statusOptions} /></div>
        <Button type="submit" variant="outline" disabled={store.loading}>{m.search()}</Button>
      </form>
    {/snippet}

    <div class="hidden min-h-0 flex-1 overflow-auto md:flex md:flex-col">
      <DataTable data={store.users} {columns} keyField="id" loading={store.loading} striped={false} class="min-h-0 flex-1 overflow-auto">
        {#snippet cell(user, key)}
          {#if key === 'username'}
            <div class="flex items-center gap-3">
              <Avatar name={user.displayName || user.username} src={user.avatarUrl} size="sm" />
              <div><p class="font-medium">{user.displayName || user.username}</p><p class="text-xs text-base-content/45">@{user.username}</p></div>
            </div>
          {:else if key === 'roles'}
            <Badge variant={user.roles.includes('admin') ? 'primary' : 'ghost'}>{user.roles.includes('admin') ? m.role_admin() : m.role_user()}</Badge>
          {:else if key === 'enabled'}
            <Badge variant={user.enabled ? 'success' : 'ghost'}>{user.enabled ? m.enabled() : m.disabled()}</Badge>
          {:else if key === 'createdAt'}
            <span class="whitespace-nowrap text-xs text-base-content/60">{store.formatDate(user.createdAt)}</span>
          {:else if key === 'email'}
            {user.email || '—'}
          {/if}
        {/snippet}
        {#snippet actions(user)}
          {@render userActions(user)}
        {/snippet}
      </DataTable>
    </div>
    <div class="min-h-0 flex-1 space-y-3 overflow-auto p-3 sm:p-4 md:hidden" data-testid="user-cards">
      {#if store.loading}
        <Skeleton count={3} />
      {:else if store.users.length === 0}
        <p class="py-8 text-center text-sm text-base-content/50">{m.users_empty()}</p>
      {:else}
        {#each store.users as user (user.id)}
          <Card bordered>
            <div class="flex items-center gap-3">
              <Avatar name={user.displayName || user.username} src={user.avatarUrl} size="sm" />
              <div class="min-w-0 flex-1"><p class="truncate font-medium">{user.displayName || user.username}</p><p class="truncate text-xs text-base-content/45">@{user.username}</p></div>
              <Badge size="sm" variant={user.enabled ? 'success' : 'ghost'}>{user.enabled ? m.enabled() : m.disabled()}</Badge>
            </div>
            <p class="mt-3 truncate text-xs text-base-content/60" title={user.email}>{user.email || '—'}</p>
            <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
              <Badge size="sm" variant={user.roles.includes('admin') ? 'primary' : 'ghost'}>{user.roles.includes('admin') ? m.role_admin() : m.role_user()}</Badge>
              {@render userActions(user)}
            </div>
          </Card>
        {/each}
      {/if}
    </div>

    {#snippet footer()}
      <Pagination page={store.usersPage} total={store.usersTotal} pageSize={store.pageSize} onchange={page => void store.loadUsers(page)} />
    {/snippet}
  </DataSurface>
</ListPageLayout>

<Modal bind:open={editorOpen} title={editing ? m.user_edit() : m.user_create()} closeOnBackdrop={!store.saving} closeOnEscape={!store.saving} size="3xl" data-testid="user-editor">
  <form onsubmit={submit}>
    <FormLayout columns={2} labelPosition="left" labelWidth="6rem">
      <FormField label={m.user_username()} required>
        <Input bind:value={username} name="username" readonly={!!editing} required minlength={3} maxlength={64} pattern="[\w\-]+" autocomplete="off" data-testid="user-username" />
      </FormField>
      <FormField label={m.user_display_name()}><Input bind:value={displayName} maxlength={100} data-testid="user-display-name" /></FormField>
      <FormField label={m.user_email()}><Input type="email" bind:value={email} data-testid="user-email" /></FormField>
      {#if !editing}
        <FormField label={m.user_password()} hint={m.initial_password_rules({ min: minPasswordLength })} required><PasswordInput required bind:value={password} autocomplete="new-password" minLength={minPasswordLength} data-testid="user-password" /></FormField>
      {/if}
      <FormField label={m.user_role()}><Select bind:value={role} options={roleOptions} disabled={editing?.id === store.user?.id} data-testid="user-role" /></FormField>
      <Checkbox bind:checked={enabled} label={m.account_enabled()} disabled={editing?.id === store.user?.id} />
      {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}
      <div class="flex justify-end gap-2 border-t border-base-content/8 pt-4">
        <Button variant="ghost" disabled={store.saving} onclick={() => { editorOpen = false }}>{m.cancel()}</Button>
        <Button type="submit" variant="primary" loading={store.saving} disabled={!canSubmit} data-testid="user-save">{m.save()}</Button>
      </div>
    </FormLayout>
  </form>
</Modal>

{#snippet userActions(user: AdminUser)}
  <div class="flex items-center gap-1" data-testid="user-actions" data-user={user.username}>
    <Button size="xs" variant="ghost" onclick={() => openEditor(user)}>{m.edit()}</Button>
    <label class="cursor-pointer rounded-lg px-2 py-1.5 text-xs text-primary hover:bg-primary/5">
      {m.avatar_upload()}
      <BareInput type="file" class="sr-only" accept="image/png,image/jpeg,image/webp" disabled={store.saving} onchange={event => upload(user, event)} data-testid="user-avatar-input" />
    </label>
    <Button size="xs" variant="ghost" class="text-error" disabled={user.id === store.user?.id || store.saving} onclick={() => { deleting = user }}>{m.delete()}</Button>
  </div>
{/snippet}

<Modal open={!!deleting} title={m.user_delete_title()} onclose={() => { deleting = null }} closeOnBackdrop={!store.saving} closeOnEscape={!store.saving}>
  <p class="text-sm leading-6 text-base-content/70">{m.user_delete_confirm({ name: deleting?.displayName || deleting?.username || '' })}</p>
  {#if store.error}<p class="mt-3 text-sm text-error" role="alert">{store.error}</p>{/if}
  {#snippet footer()}
    <Button variant="ghost" disabled={store.saving} onclick={() => { deleting = null }}>{m.cancel()}</Button>
    <Button variant="error" loading={store.saving} onclick={confirmDelete} data-testid="confirm-delete-user">{m.delete()}</Button>
  {/snippet}
</Modal>
