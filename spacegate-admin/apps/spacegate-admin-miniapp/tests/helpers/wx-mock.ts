/** 一次 `wx.request` 调用的快照，供断言 URL、方法、头和 body。 */
export interface MockCall {
  url: string
  method: string
  header: Record<string, string>
  data: unknown
}

/** mock 响应；不抛异常时作为成功响应返回。 */
export interface MockResponse {
  statusCode?: number
  data: unknown
}

const pending: MockCall[] = []

/** 当前 mock 收集到的调用记录。 */
export function recordedCalls(): MockCall[] {
  return pending
}

/** 清空调用记录。 */
export function clearCalls(): void {
  pending.length = 0
}

/**
 * 安装全局 `wx` mock。
 * @param handler - 按调用返回响应；可异步模拟延迟
 * @returns 卸载函数，恢复全局 `wx` 为 undefined
 */
export function installWx(handler: (call: MockCall) => MockResponse | Promise<MockResponse>): () => void {
  const api: WxApi = {
    request<T>(options: WxRequestOptions<T>): WxRequestTask {
      const call: MockCall = {
        url: options.url,
        method: options.method ?? 'GET',
        header: options.header ?? {},
        data: options.data,
      }
      pending.push(call)
      Promise.resolve()
        .then(() => handler(call))
        .then((response) => {
          options.success?.({ data: response.data as T, statusCode: response.statusCode ?? 200, header: {} })
        })
        .catch((error: unknown) => {
          options.fail?.({ errMsg: error instanceof Error ? error.message : 'mock request failed' })
        })
      return { abort: () => undefined }
    },
    showToast: () => undefined,
    showModal: options => options?.success?.({ confirm: true, cancel: false }),
    showLoading: () => undefined,
    hideLoading: () => undefined,
    setNavigationBarTitle: () => undefined,
    navigateTo: () => undefined,
    redirectTo: () => undefined,
    reLaunch: () => undefined,
    navigateBack: () => undefined,
  }
  ;(globalThis as unknown as { wx: WxApi }).wx = api
  return () => {
    Reflect.deleteProperty(globalThis, 'wx')
  }
}

/** 成功 HaiResult。 */
export function ok<T>(data: T): MockResponse {
  return { data: { success: true, data } }
}

/** 失败 HaiResult；statusCode 默认 200，与服务端业务错误返回方式一致。 */
export function fail(httpStatus: number, message: string, code = 'TEST_ERROR'): MockResponse {
  return { data: { success: false, error: { code, message, httpStatus } } }
}

/** 登录成功的最小 IAM 返回。 */
export function loginPayload(accessToken = 'access-1', refreshToken = 'refresh-1') {
  return {
    user: { id: 'user-1', username: 'admin', displayName: 'Admin' },
    tokens: { accessToken, refreshToken, expiresIn: 3600, tokenType: 'Bearer' as const },
    roles: ['admin'],
    permissions: ['*'],
  }
}
