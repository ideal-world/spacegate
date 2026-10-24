<script lang="ts">
  import { FormField } from '@h-ai/ui/components/compounds/index'
  import { Button, Input, Select, Switch } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'
  import { asRecord, cloneRecord } from '../../spacegate/model.js'
  import {
    enumOptions,
    enumValueFromToken,
    enumValueToken,
    initSchemaDefaults,
    newSchemaArrayItem,
    parseNumberInput,
    resolveSchema,
    schemaBranches,
    schemaDescription,
    schemaInputValue,
    schemaItems,
    schemaKind,
    schemaProperties,
    schemaRequired,
    schemaTitle,
  } from '../../spacegate/schema.js'
  import JsonTextarea from './JsonTextarea.svelte'
  import SchemaForm from './SchemaForm.svelte'

  interface Props {
    schema: unknown
    value?: unknown
    root?: unknown
    label?: string
    path?: string
    required?: boolean
    disabled?: boolean
    depth?: number
    onchange: (value: unknown) => void
  }
  const { schema, value, root = schema, label, path, required = false, disabled = false, depth = 0, onchange }: Props = $props()

  let branchIndex = $state(0)
  const resolved = $derived(resolveSchema(schema, root))
  const kind = $derived(schemaKind(resolved, root))
  const title = $derived(label ?? schemaTitle(resolved, path ?? '', root))
  const description = $derived(schemaDescription(resolved, root))
  const requiredNames = $derived(schemaRequired(resolved, root))
  const properties = $derived(schemaProperties(resolved, root))
  const options = $derived(enumOptions(resolved, root))
  const branches = $derived(schemaBranches(resolved, root))
  const itemSchema = $derived(schemaItems(resolved, root))
  const currentRecord = $derived(asRecord(value) ?? {})
  const branch = $derived(branches[branchIndex] ?? branches[0] ?? {})

  function stringText(value: unknown): string {
    return typeof value === 'string' ? value : typeof value === 'number' ? String(value) : ''
  }

  function setField(key: string, next: unknown) {
    const record = cloneRecord(value)
    record[key] = next
    onchange(record)
  }

  function setArrayItem(index: number, next: unknown) {
    const list = Array.isArray(value) ? [...value] : []
    list[index] = next
    onchange(list)
  }

  function addArrayItem() {
    const list = Array.isArray(value) ? [...value] : []
    list.push(initSchemaDefaults(itemSchema, newSchemaArrayItem(itemSchema, root), root))
    onchange(list)
  }

  function removeArrayItem(index: number) {
    if (!Array.isArray(value))
      return
    onchange(value.filter((_, itemIndex) => itemIndex !== index))
  }

  function chooseBranch(token: string) {
    const index = Number(token)
    if (!Number.isInteger(index) || !branches[index])
      return
    branchIndex = index
    onchange(initSchemaDefaults(branches[index], value, root))
  }
</script>

{#if depth > 10}
  <JsonTextarea {value} rows={8} label={title} {disabled} onchange={onchange} />
{:else if kind === 'object'}
  {#if properties.length === 0}
    <JsonTextarea {value} rows={8} label={title} {disabled} onchange={onchange} />
  {:else}
    <div class="space-y-3" data-testid="schema-object">
      {#if label}
        <div>
          <p class="text-sm font-medium">{label}{#if required}<span class="ml-1 text-error">*</span>{/if}</p>
          {#if description}<p class="mt-1 text-xs text-base-content/55">{description}</p>{/if}
        </div>
      {/if}
      {#each properties as [key, property] (key)}
        <SchemaForm
          schema={property}
          root={root}
          value={currentRecord[key]}
          path={key}
          label={schemaTitle(property, key, root)}
          required={requiredNames.has(key)}
          {disabled}
          depth={depth + 1}
          onchange={(next: unknown) => setField(key, next)}
        />
      {/each}
    </div>
  {/if}
{:else if kind === 'array'}
  <div class="space-y-3" data-testid="schema-array">
    <div class="flex items-center justify-between gap-2">
      <p class="text-sm font-medium">{title}{#if required}<span class="ml-1 text-error">*</span>{/if}</p>
      <Button size="xs" variant="outline" {disabled} onclick={addArrayItem}>{m.schema_add_item()}</Button>
    </div>
    {#if description}<p class="text-xs text-base-content/55">{description}</p>{/if}
    {#if !Array.isArray(value) || value.length === 0}
      <p class="text-xs text-base-content/45">{m.schema_empty_array()}</p>
    {:else}
      {#each value as item, index (index)}
        <div class="rounded-lg border border-base-content/10 p-3">
          <div class="mb-2 flex items-center justify-between gap-2">
            <span class="text-xs text-base-content/50">{m.schema_item({ index: String(index + 1) })}</span>
            <Button size="xs" variant="ghost" class="text-error" {disabled} onclick={() => removeArrayItem(index)}>{m.remove()}</Button>
          </div>
          <SchemaForm
            schema={itemSchema}
            root={root}
            value={item}
            {disabled}
            depth={depth + 1}
            onchange={(next: unknown) => setArrayItem(index, next)}
          />
        </div>
      {/each}
    {/if}
  </div>
{:else if kind === 'enum'}
  <FormField label={title} hint={description} {required}>
    <Select
      value={value === undefined ? '' : enumValueToken(value)}
      options={options.map(option => ({ value: enumValueToken(option.value), label: option.label }))}
      placeholder={m.select_placeholder()}
      clearable={!required}
      {disabled}
      onchange={token => onchange(token ? enumValueFromToken(token) : undefined)}
    />
  </FormField>
{:else if kind === 'boolean'}
  <FormField label={title} hint={description}>
    <Switch checked={value === true} {disabled} onchange={checked => onchange(checked)} />
  </FormField>
{:else if kind === 'number' || kind === 'integer'}
  <FormField label={title} hint={description} {required}>
    <Input
      type="number"
      step={kind === 'integer' ? 1 : 0.01}
      value={schemaInputValue(value)}
      {disabled}
      onchange={event => onchange(parseNumberInput(event.currentTarget.value, kind === 'integer'))}
    />
  </FormField>
{:else if kind === 'string'}
  <FormField label={title} hint={description} {required}>
    <Input value={stringText(value)} {disabled} required={required} oninput={event => onchange(event.currentTarget.value)} />
  </FormField>
{:else if kind === 'oneOf'}
  <div class="space-y-3" data-testid="schema-one-of">
    <FormField label={title} hint={description}>
      <Select
        value={String(branchIndex)}
        options={branches.map((item, index) => ({ value: String(index), label: schemaTitle(item, m.schema_variant({ index: String(index + 1) }), root) }))}
        {disabled}
        onchange={chooseBranch}
      />
    </FormField>
    <SchemaForm
      schema={branch}
      root={root}
      value={value}
      {disabled}
      depth={depth + 1}
      onchange={onchange}
    />
  </div>
{:else}
  <JsonTextarea {value} rows={8} label={title || m.schema_value()} {disabled} onchange={onchange} />
{/if}
