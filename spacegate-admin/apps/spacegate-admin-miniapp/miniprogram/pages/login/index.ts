import type { MiniPublicConfig } from '../../lib/api'
import { hasSession, login, loginWithLdap, publicConfig, UnauthorizedError } from '../../lib/api'
import { getLocale, labelsFor, localeOptions, setLocale, t } from '../../lib/i18n'
import { goOverview, setPageTitle } from '../../lib/nav'
import { describeError } from '../../lib/ui'

const PAGE_LABELS = [
  'login_title',
  'login_description',
  'mini_login_identifier',
  'mini_login_identifier_placeholder',
  'mini_login_password',
  'mini_login_submit',
  'mini_login_ldap',
  'mini_login_ldap_hint',
  'mini_login_password_mode',
  'mini_login_no_ldap',
  'mini_required',
  'mini_language',
  'mini_app_name',
] as const

function localeIndex(): number {
  const options = localeOptions()
  const index = options.findIndex(option => option.value === getLocale())
  return index < 0 ? 0 : index
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    locales: localeOptions(),
    localeIndex: localeIndex(),
    identifier: '',
    password: '',
    ldapMode: false,
    ldapEnabled: false,
    loading: false,
    error: '',
  },
  onLoad() {
    setPageTitle(t('mini_app_name'))
    void this.loadConfig()
  },
  onShow() {
    if (hasSession())
      goOverview()
  },
  async loadConfig() {
    try {
      const config: MiniPublicConfig | null = await publicConfig()
      this.setData({ ldapEnabled: Boolean(config?.login.ldap) })
    }
    catch {
      // 公开配置失败不阻止密码登录尝试；错误在提交时反馈。
    }
  },
  onIdentifierInput(event: WxInputEvent) {
    this.setData({ identifier: event.detail.value })
  },
  onPasswordInput(event: WxInputEvent) {
    this.setData({ password: event.detail.value })
  },
  onToggleMode() {
    if (!this.data.ldapMode && !this.data.ldapEnabled) {
      this.setData({ error: this.data.labels.mini_login_no_ldap })
      return
    }
    this.setData({ ldapMode: !this.data.ldapMode, error: '' })
  },
  onCycleLocale() {
    const options = localeOptions()
    const next = (this.data.localeIndex + 1) % options.length
    const option = options[next]
    if (!option)
      return
    setLocale(option.value)
    setPageTitle(t('mini_app_name'))
    this.setData({ localeIndex: next, labels: labelsFor(PAGE_LABELS), error: '' })
  },
  async onSubmit() {
    if (this.data.loading)
      return
    const identifier = this.data.identifier.trim()
    if (!identifier || !this.data.password) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    this.setData({ loading: true, error: '' })
    try {
      if (this.data.ldapMode)
        await loginWithLdap(identifier, this.data.password)
      else
        await login(identifier, this.data.password)
      goOverview()
    }
    catch (error) {
      const message = error instanceof UnauthorizedError ? t('login_invalid') : describeError(error)
      this.setData({ error: message })
    }
    finally {
      this.setData({ loading: false })
    }
  },
})
