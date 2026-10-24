<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import type { JsonRecord } from '../../spacegate/model.js'
  import { FormField, FormLayout, Modal } from '@h-ai/ui/components/compounds/index'
  import { Button, Input, Select } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import { cloneRecord, readNumber, readString } from '../../spacegate/model.js'

  interface Props {
    store: ManagementStore
    open?: boolean
    rule?: JsonRecord | null
  }
  let { store, open = $bindable(false), rule = null }: Props = $props()

  let tenant = $state('')
  let model = $state('')
  let path = $state('')
  let policy = $state('')
  let rps = $state(10)
  let burst = $state(20)
  let cost = $state(1)
  let ttl = $state('')
  let validation = $state('')
  let initializedFor = $state<string>('')

  const policyOptions = [{ value: '', label: m.ai_policy_any() }, { value: 'abandon', label: 'abandon' }, { value: 'queue', label: 'queue' }, { value: 'wait', label: 'wait' }]

  $effect(() => {
    if (!open) {
      initializedFor = ''
      return
    }
    const key = rule ? readString(rule, 'key') || JSON.stringify(rule) : ''
    if (initializedFor === key)
      return
    initializedFor = key
    tenant = readString(rule, 'tenant')
    model = readString(rule, 'model')
    path = readString(rule, 'path')
    policy = readString(rule, 'policy')
    rps = readNumber(rule, 'rps') ?? 10
    burst = readNumber(rule, 'burst') ?? 20
    cost = readNumber(rule, 'cost') ?? 1
    ttl = readNumber(rule, 'ttl_secs') === undefined ? '' : String(readNumber(rule, 'ttl_secs'))
    validation = ''
    store.error = ''
  })

  function validate(): boolean {
    const tenantValue = tenant.trim()
    if (!tenantValue || tenantValue.includes(':')) {
      validation = m.ai_validation_tenant()
      return false
    }
    if (model.trim().includes(':') || path.trim().includes(':')) {
      validation = m.ai_validation_separator()
      return false
    }
    if (!(rps > 0) || !(burst > 0) || !(cost > 0)) {
      validation = m.ai_validation_number()
      return false
    }
    if (cost > burst) {
      validation = m.ai_validation_cost()
      return false
    }
    if (ttl.trim() && !(Number(ttl) > 0)) {
      validation = m.ai_validation_ttl()
      return false
    }
    return true
  }

  async function save() {
    validation = ''
    if (!validate())
      return
    const next = cloneRecord(rule)
    next.tenant = tenant.trim()
    if (model.trim())
      next.model = model.trim()
    else
      delete next.model
    if (path.trim())
      next.path = path.trim()
    else
      delete next.path
    if (policy)
      next.policy = policy
    else
      delete next.policy
    next.rps = rps
    next.burst = burst
    next.cost = cost
    if (ttl.trim())
      next.ttl_secs = Number(ttl)
    else
      delete next.ttl_secs
    delete next.key
    delete next.ttl_remaining_secs
    if (await store.saveAiRule(next))
      open = false
  }
</script>

<Modal bind:open title={rule ? m.ai_rule_edit() : m.ai_rule_create()} size="2xl" closeOnBackdrop={!store.saving} closeOnEscape={!store.saving} data-testid="ai-rule-editor">
  <div class="space-y-5">
    <FormLayout columns={2}>
      <FormField label={m.ai_tenant()} required>
        <Input bind:value={tenant} required oninput={() => { validation = '' }} data-testid="ai-rule-tenant" />
      </FormField>
      <FormField label={m.ai_model()}>
        <Input bind:value={model} placeholder={m.ai_policy_any()} oninput={() => { validation = '' }} />
      </FormField>
      <FormField label={m.ai_path()}>
        <Input bind:value={path} placeholder="/v1/chat" oninput={() => { validation = '' }} />
      </FormField>
      <FormField label={m.ai_policy()}>
        <Select bind:value={policy} options={policyOptions} />
      </FormField>
      <FormField label={m.ai_rps()} required>
        <Input type="number" min={1} step={1} value={String(rps)} oninput={(event) => {
          rps = Number(event.currentTarget.value) || 0
          validation = ''
        }} />
      </FormField>
      <FormField label={m.ai_burst()} required>
        <Input type="number" min={1} step={1} value={String(burst)} oninput={(event) => {
          burst = Number(event.currentTarget.value) || 0
          validation = ''
        }} />
      </FormField>
      <FormField label={m.ai_cost()} required>
        <Input type="number" min={1} step={1} value={String(cost)} oninput={(event) => {
          cost = Number(event.currentTarget.value) || 0
          validation = ''
        }} />
      </FormField>
      <FormField label={m.ai_ttl()}>
        <Input type="number" min={1} step={1} bind:value={ttl} placeholder={m.ai_ttl_placeholder()} oninput={() => { validation = '' }} />
      </FormField>
    </FormLayout>
    {#if validation}<p class="text-sm text-error" role="alert">{validation}</p>{/if}
    {#if store.error}<p class="text-sm text-error" role="alert">{store.error}</p>{/if}
    <div class="flex justify-end gap-2 border-t border-base-content/8 pt-4">
      <Button variant="ghost" disabled={store.saving} onclick={() => { open = false }}>{m.cancel()}</Button>
      <Button variant="primary" loading={store.saving} onclick={() => void save()} data-testid="ai-rule-save">{m.save()}</Button>
    </div>
  </div>
</Modal>
