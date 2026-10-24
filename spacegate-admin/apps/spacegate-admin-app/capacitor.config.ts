import type { CapacitorConfig } from '@capacitor/cli'
import process from 'node:process'

const config: CapacitorConfig = {
  appId: 'com.hai.spacegateadmin.app',
  appName: 'spacegate-admin',
  webDir: 'dist',
  server: {
    // 仅用于 Capacitor live reload 前端页面；API 地址由前端配置或 shared client 参数控制。
    url: process.env.CAPACITOR_SERVER_URL,
    cleartext: process.env.CAPACITOR_SERVER_URL?.startsWith('http://') ?? false,
  },
}

export default config
