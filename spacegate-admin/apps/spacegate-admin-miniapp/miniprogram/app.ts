import { getLocale, setLocale } from './lib/i18n'

/** 小程序启动参数；API 基地址可由 ext 配置覆盖，token 始终只保存在内存。 */
App({
  globalData: {
    locale: getLocale(),
    currentGateway: '',
  },
  onLaunch() {
    // 语言只作为界面偏好保存在进程内；token 不允许写入 storage。
    const app = getApp<{ globalData: { locale: string } }>()
    setLocale(app.globalData.locale)
  },
})
