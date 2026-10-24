<script lang="ts">
  import { AdminWorkspace } from 'spacegate-admin-shared'
  import { setLocale } from './lib/paraglide/runtime.js'

  const apiOptions = { clientName: 'spacegate-admin-web' }

  function syncLocale(locale: string) {
    setLocale(locale as Parameters<typeof setLocale>[0], { reload: false })
  }
</script>

<AdminWorkspace platform="web" {apiOptions} onLocaleChange={syncLocale} />
