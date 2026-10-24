import type { MiniCurrentUser } from '../../lib/api'
import { changePassword, currentUser, logout, updateProfile } from '../../lib/api'
import { getLocale, labelsFor, localeOptions, setLocale, t } from '../../lib/i18n'
import { backToOverview, ensureSession, goLogin, setPageTitle } from '../../lib/nav'
import { confirmAction, describeError, toastDone } from '../../lib/ui'

const PAGE_LABELS = [
  'mini_nav_profile',
  'mini_back',
  'mini_section_profile',
  'mini_section_security',
  'mini_account',
  'mini_roles',
  'user_display_name',
  'user_email',
  'mini_button_save',
  'mini_old_password',
  'mini_new_password',
  'mini_button_change_password',
  'mini_button_logout',
  'mini_language',
  'home_refresh',
  'mini_profile_updated',
  'mini_password_changed',
  'mini_language_saved',
  'mini_logout_confirm',
  'mini_logout_warning',
  'mini_required',
  'mini_danger_zone',
  'password_rules',
] as const

function currentLocaleIndex(): number {
  const options = localeOptions()
  const index = options.findIndex(option => option.value === getLocale())
  return index < 0 ? 0 : index
}

Page({
  data: {
    labels: labelsFor(PAGE_LABELS),
    locales: localeOptions(),
    localeIndex: currentLocaleIndex(),
    username: '',
    roles: [] as string[],
    displayName: '',
    email: '',
    oldPassword: '',
    newPassword: '',
    loading: false,
    saving: false,
    error: '',
  },
  onShow() {
    if (!ensureSession())
      return
    setPageTitle(t('mini_nav_profile'))
    this.setData({ labels: labelsFor(PAGE_LABELS), locales: localeOptions(), localeIndex: currentLocaleIndex() })
    void this.load()
  },
  async load() {
    this.setData({ loading: true, error: '' })
    try {
      const user: MiniCurrentUser = await currentUser()
      this.setData({
        username: user.username,
        roles: user.roles,
        displayName: user.displayName ?? '',
        email: user.email ?? '',
      })
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ loading: false })
    }
  },
  onDisplayNameInput(event: WxInputEvent) {
    this.setData({ displayName: event.detail.value })
  },
  onEmailInput(event: WxInputEvent) {
    this.setData({ email: event.detail.value })
  },
  onOldPasswordInput(event: WxInputEvent) {
    this.setData({ oldPassword: event.detail.value })
  },
  onNewPasswordInput(event: WxInputEvent) {
    this.setData({ newPassword: event.detail.value })
  },
  async onSaveProfile() {
    if (this.data.saving)
      return
    this.setData({ saving: true, error: '' })
    try {
      await updateProfile({ displayName: this.data.displayName.trim(), email: this.data.email.trim() })
      toastDone(t('mini_profile_updated'))
      await this.load()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onChangePassword() {
    if (this.data.saving)
      return
    if (!this.data.oldPassword || !this.data.newPassword) {
      this.setData({ error: this.data.labels.mini_required })
      return
    }
    if (this.data.oldPassword === this.data.newPassword) {
      this.setData({ error: t('password_rules') })
      return
    }
    this.setData({ saving: true, error: '' })
    try {
      await changePassword(this.data.oldPassword, this.data.newPassword)
      toastDone(t('mini_password_changed'))
      await logout()
      goLogin()
    }
    catch (error) {
      this.setData({ error: describeError(error) })
    }
    finally {
      this.setData({ saving: false })
    }
  },
  async onLocaleChange(event: WxPickerEvent) {
    const index = Number(event.detail.value)
    const option = this.data.locales[index]
    if (!option)
      return
    setLocale(option.value)
    setPageTitle(t('mini_nav_profile'))
    this.setData({ localeIndex: index, labels: labelsFor(PAGE_LABELS), locales: localeOptions() })
    toastDone(t('mini_language_saved'))
    try {
      await updateProfile({ locale: option.value })
    }
    catch {
      // 语言偏好同步失败不回滚当前会话界面，避免影响退出等关键操作。
    }
  },
  async onLogout() {
    const confirmed = await confirmAction(t('mini_logout_confirm'), t('mini_logout_warning'), t('mini_button_logout'))
    if (!confirmed)
      return
    await logout()
    goLogin()
  },
  onBack() {
    backToOverview()
  },
})
