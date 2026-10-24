import { core } from '@h-ai/core'

/** 服务端业务提示与角色种子名称同步维护中英日文。 */
export const serverM = core.i18n.createMessageGetter({
  'zh-CN': {
    admin: '管理员',
    user: '普通用户',
    adminDescription: '管理用户、角色和审计日志',
    userDescription: '仅可维护个人设置',
    notFound: '用户或角色不存在',
    unauthorized: '请提供要撤销的访问令牌',
    selfProtection: '不能删除、禁用或降级当前管理员账号',
    invalidAvatar: '请选择不超过 512 KB 的有效 PNG、JPEG 或 WebP 图片',
    busy: '系统繁忙，请稍后重试',
    unsupportedPreference: '不支持此语言或主题',
    invalidDateRange: '结束时间不能早于开始时间',
  },
  'ja-JP': {
    admin: '管理者',
    user: 'ユーザー',
    adminDescription: 'ユーザー、ロール、監査ログを管理',
    userDescription: '個人設定のみ管理',
    notFound: 'ユーザーまたはロールが存在しません',
    unauthorized: '取り消すアクセストークンを指定してください',
    selfProtection: '自分の管理者アカウントは削除、無効化、降格できません',
    invalidAvatar: '512KB以下の有効なPNG、JPEG、WebP画像を選択してください',
    busy: 'システムが混雑しています。再試行してください',
    unsupportedPreference: 'この言語またはテーマには対応していません',
    invalidDateRange: '終了日は開始日以降にしてください',
  },
  'en-US': {
    admin: 'Administrator',
    user: 'User',
    adminDescription: 'Manage users, roles and audit logs',
    userDescription: 'Manage personal settings only',
    notFound: 'User or role does not exist',
    unauthorized: 'Provide the access token to revoke',
    selfProtection: 'You cannot delete, disable or demote your own administrator account',
    invalidAvatar: 'Choose a valid PNG, JPEG or WebP image up to 512 KB',
    busy: 'The system is busy. Please try again',
    unsupportedPreference: 'This language or theme is not supported',
    invalidDateRange: 'End date must not precede start date',
  },
})
