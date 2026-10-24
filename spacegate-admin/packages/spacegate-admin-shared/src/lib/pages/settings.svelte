<script lang='ts'>
  import type { ManagementStore } from '../stores/management-store.svelte.js'
  import { Card, FormField, FormLayout } from '@h-ai/ui/components/compounds/index'
  import { Badge, Select } from '@h-ai/ui/components/primitives/index'
  import { ChangePasswordForm, UserProfile } from '@h-ai/ui/components/scenes/iam/index'
  import { getThemeInfo } from '@h-ai/ui/theme'
  import * as m from '../paraglide/messages.js'
  import { locales } from '../paraglide/runtime.js'
  import { AVAILABLE_THEMES, themeStore } from '../stores/theme-store.svelte.js'

  interface Props { store: ManagementStore }
  const { store }: Props = $props()
  const languageOptions = $derived(locales.filter(locale => store.info?.supportedLocales.includes(locale)).map(locale => ({ value: locale, label: locale === 'zh-CN' ? '简体中文' : locale === 'ja-JP' ? '日本語' : 'English' })))
  const themeOptions = AVAILABLE_THEMES.map(theme => ({ value: theme, label: getThemeInfo(theme)?.name ?? theme }))
</script>

<section class='grid w-full min-w-0 gap-6 xl:grid-cols-2' data-testid='settings-page'>
  <div class='xl:col-span-2'>
    <h1 class='text-xl font-semibold tracking-tight'>{m.settings_title()}</h1>
    <p class='mt-1 text-sm text-base-content/50'>{m.settings_description()}</p>
  </div>
  {#if store.user}
    <Card class='xl:row-span-2' title={m.profile_title()} bordered shadow={false} data-guide='admin-profile-form'>
      <div class='mb-5 flex flex-wrap items-center gap-3 border-b border-base-content/8 pb-4'>
        <p class='min-w-0 break-all text-sm text-base-content/55'>@{store.user.username}</p>
        <Badge size='sm' variant='ghost'>{store.isAdmin ? m.role_admin() : m.role_user()}</Badge>
      </div>
      <UserProfile columns={2} user={store.user} editable alwaysEditable fields={['avatar', 'displayName', 'email']} avatarAccept='image/png,image/jpeg,image/webp' avatarHint={m.avatar_hint()} loading={store.saving} onsubmit={data => store.saveProfile(data)} onavatarchange={file => void store.uploadAvatar(file)} data-testid='profile-form' />
    </Card>
    <Card title={m.preferences_title()} bordered shadow={false} data-guide='admin-preferences'>
      <p class='mb-5 text-sm text-base-content/50'>{m.preferences_description()}</p>
      <FormLayout columns={2}>
        <FormField label={m.settings_language()} for='profile-language'>
          <Select id='profile-language' value={store.locale} options={languageOptions} disabled={store.saving} onchange={locale => void store.savePreferences({ locale })} data-testid='profile-language' />
        </FormField>
        <FormField label={m.settings_theme()} for='profile-theme'>
          <Select id='profile-theme' value={themeStore.current} options={themeOptions} disabled={store.saving} onchange={theme => void store.savePreferences({ theme })} data-testid='profile-theme' />
        </FormField>
      </FormLayout>
    </Card>
    <Card title={m.password_title()} bordered shadow={false} data-guide='admin-password'>
      <p class='mb-5 text-sm text-base-content/50'>{m.password_rules()}</p>
      <ChangePasswordForm loading={store.saving} passwordPolicy={store.authConfig?.password} minPasswordLength={store.authConfig?.password.minLength ?? 8} onsubmit={data => store.changePassword(data)} data-testid='password-form' />
    </Card>
  {/if}
</section>
