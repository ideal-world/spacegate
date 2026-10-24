<script lang="ts">
  import type { ManagementStore } from '../../stores/management-store.svelte.js'
  import { Alert } from '@h-ai/ui/components/compounds/index'
  import { Button } from '@h-ai/ui/components/primitives/index'
  import * as m from '../../paraglide/messages.js'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()
</script>

{#if store.conflict}
  <Alert variant="warning" title={m.version_conflict_title()} data-testid="version-conflict">
    <div class="space-y-3">
      <p class="text-sm leading-6">{m.version_conflict({ version: String(store.conflictServerVersion ?? '?') })}</p>
      <p class="text-xs text-base-content/60">{m.version_conflict_hint()}</p>
      <Button size="sm" variant="outline" loading={store.loading} onclick={() => void store.reloadAfterConflict()} data-testid="version-conflict-reload">
        {m.version_conflict_reload()}
      </Button>
    </div>
  </Alert>
{/if}
