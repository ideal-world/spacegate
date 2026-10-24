# hai-payment 参考

> `@h-ai/payment` 是 hai-framework 的统一支付模块，通过 Provider 模式支持微信支付、支付宝、Stripe。服务端处理订单和回调，客户端调起支付。

公开类型示例按实际包入口校验：初始化输入为 `PaymentConfigInput`，配置 Schema 输出为 `PaymentConfig`；`HaiPaymentError` 的单个成员是错误定义，比较 `result.error.code` 时使用其 `.code`。私有协议、签名、操作仓库不属于应用导入入口。JSDoc 的类型校验不会执行渠道支付或业务履约。

## 运行环境

> **服务端 + 浏览器分工模块。** `payment.createOrder` / `payment.handleNotify` / `payment.refund` 等操作在 Node.js 端执行。浏览器端使用 `invokePayment()`（从 `@h-ai/payment/client` 导入）调起支付。

## 依赖

| 模块          | 用途                                | 是否必需           | 初始化要求                               |
| ------------- | ----------------------------------- | ------------------ | ---------------------------------------- |
| `@h-ai/audit` | 审计日志（支付操作自动审计）        | 可选               | 已初始化时自动写入审计日志               |
| `@h-ai/reldb` | Stripe 渠道映射、操作预留与幂等结果 | 启用 Stripe 时必需 | 先初始化共享 DB；payment 关闭后再关闭 DB |

## 适用场景

- 接入微信公众号 JSAPI 支付（需 openid/WeixinJSBridge）
- 接入支付宝 H5（WAP 离线签名表单）
- 接入 Stripe（Checkout Session）
- 支付回调验签与通知处理
- 订单查询与退款
- 客户端调起支付（微信内置浏览器 / H5 跳转）

## 使用步骤

### 1. 配置

```yaml
# config/_payment.yml（自行创建）
wechat:
  appId: ''
  mchId: ''
  apiV3Key: ''
  privateKey: ''
  serialNo: ''
  platformKeys: # 必填，按通知 Wechatpay-Serial 精确选择受信平台证书/公钥
    PUB_KEY_ID_123: '' # 平台公钥 PEM；证书使用其真实序列号作为 key

alipay:
  appId: ''
  sellerId: '' # 必填，验签后核对 seller_id
  privateKey: ''
  alipayPublicKey: ''
  returnUrl: 'https://app.example/payment/return' # 必填 HTTPS，不能作为付款证明
  signType: RSA2 # 可选，默认 RSA2
  sandbox: false # 可选，默认 false

stripe:
  secretKey: ''
  webhookSecret: ''
  successUrl: 'https://app.example/payment/success?session_id={CHECKOUT_SESSION_ID}' # 必填
  cancelUrl: 'https://app.example/payment/cancel' # 可选
  webhookToleranceSeconds: 300 # 可选，默认 300 秒
```

Stripe 的 secretKey 必须为 sk/rk 的 test/live 服务端密钥；受限密钥须允许读取账户。只实现 payment + hosted_page + tradeType=h5，必填 successUrl、可选 cancelUrl 均来自服务端配置且必须为 HTTP(S)。不支持 embedded/elements 或 App/JSAPI/Native 等支付场景，不向客户端提供改变模式/跳转地址的接口。API 固定为 2026-09-30.endive；启用前初始化共享 reldb，不能给不同节点配置各自的 SQLite 文件。

内置创建模式仅保留上表组合；公开 `TradeType` 为 `jsapi | h5`，App、Native、小程序及其它组合均在请求渠道前返回 `UNSUPPORTED_TRADE_TYPE`。微信 JSAPI 必须提供当前公众号的 `userId`（openid），微信与支付宝仅接受 CNY。微信下单缺失有效 `prepay_id` 时失败。支付宝 `createOrder` 只生成已签名的页面请求参数，不表示渠道已受理或已付款；返回 `{ gateway, fields }`，浏览器向固定正式/沙箱网关 POST 全部签名字段，不解析 HTML，也不接收任意 action。配置 `alipay.returnUrl` 必填且为 HTTPS，`HAI_PAYMENT_ALIPAY_RETURNURL` 可覆盖；回跳不代表付款成功。客户端先检查 `success`，再检查 `data.invoked`；微信桥接对象未就绪时 `invoked=false`，已调起后的取消回调也不代表付款。

协议依据：[微信 JSAPI 下单](https://pay.wechatpay.cn/doc/v3/merchant/4012791897)、[JSAPI 调起](https://pay.wechatpay.cn/doc/v3/merchant/4012791857)、[支付宝官方 Node SDK pageExecute](https://github.com/alipay/alipay-sdk-nodejs-all#pageexecute-示例代码)。定向测试验证请求/签名形状、真实 RSA 验签和 jsdom 表单提交；未使用真实商户凭据、渠道沙箱账户或手机设备，不能据此宣称已完成真实支付验收。

约定环境变量优先于 YAML，例如 `HAI_PAYMENT_WECHAT_APIV3KEY`、
`HAI_PAYMENT_STRIPE_WEBHOOKSECRET`。生产密钥只放环境或密钥管理器。

### 2. 初始化（服务端）

```typescript
import { payment } from '@h-ai/payment'

const initResult = await payment.init({
  wechat: {
    appId: 'wx1234567890',
    mchId: '1600000000',
    apiV3Key: 'your-api-key-v3',
    privateKey: '-----BEGIN RSA PRIVATE KEY-----...',
    serialNo: 'CERT_SERIAL_NO',
    platformKeys: { PUB_KEY_ID_123: '-----BEGIN PUBLIC KEY-----...' },
  },
  alipay: {
    appId: '2021000000000000',
    sellerId: '2088000000000000',
    privateKey: '-----BEGIN RSA PRIVATE KEY-----...',
    alipayPublicKey: '-----BEGIN PUBLIC KEY-----...',
    returnUrl: 'https://app.example/payment/return',
  },
})
if (!initResult.success) {
  // 处理初始化错误
}
```

### 3. 创建订单

```typescript
const result = await payment.createOrder('wechat', {
  orderNo: 'ORDER_20250101_001',
  amount: 9900, // 单位：分
  description: '商品名称',
  tradeType: 'jsapi',
  userId: 'user_openid',
  notifyUrl: 'https://example.com/payment/notify/wechat',
  metadata: { source: 'web' },
})

if (result.success) {
  // result.data: PaymentOrder { provider, tradeType, clientParams, prepayId? }
  // clientParams 包含调起支付所需参数
}

// amount / totalAmount 必须是正整数（单位：分）
```

### 4. 处理支付回调

```typescript
// API 路由：POST /payment/notify/:provider
const result = await payment.handleNotify('wechat', {
  body: await request.text(),
  headers: Object.fromEntries(request.headers),
})

if (result.success) {
  // success 仅表示验签解析成功；Stripe completed/unpaid 仍是 pending。
  // 在共享数据库事务中核对渠道、订单、amount、currency，按渠道/商户命名空间持久去重 eventId。
  // 仅 status=paid 可履约；乱序 pending/failed 不得回退已支付/已退款订单。
}
```

Stripe 只消费 payment-mode Checkout Session 的 `completed`、`async_payment_succeeded`、`async_payment_failed`、`expired` 事件，其它事件或缺失订单号/金额/币种返回 `NOTIFY_PARSE_FAILED`。结果的 `eventId` 用于持久去重，`currency` 为大写，`transactionId` 为 Session ID。只有 paid 结果返回 `paidAt`，取成功事件的 created 时间；Provider 不维护业务订单或进程内去重集合。

所有 Provider（含自定义实现）都必须返回 eventId/currency：微信 id、支付宝 notify_id、Stripe event.id；支付宝 currency 固定 CNY。微信只处理 TRANSACTION.SUCCESS/transaction，验签后继续校验 appid/mchid，按 Wechatpay-Serial 精确选择 platformKeys 项，检查证书序列号及有效期、严格整数签名时钟与当前节点相差不超过 300 秒。轮换期间所有节点同时保存新旧项；未知 serial 不能自动下载并信任。支付宝核对 app_id/seller_id/配置签名算法，拒绝重复表单字段，按北京时间严格解析真实日期；notify_id 跨日重试不变，不能对原始通知时间套五分钟有效期。支付宝请求使用 Asia/Shanghai 和 POST form body，不能把签名/订单内容放 URL。Stripe 按密钥校验 test/live，支持轮换期多个 v1 签名，只支持当前账户，拒绝 Connect/组织 context 事件。

接入应用应把“业务订单 pending→paid 的条件更新 + 唯一 eventId 记录 + 唯一履约 outbox”放进共享数据库同一个事务；所有 HaiResult 失败必须抛错使 reldb.tx.wrap 回滚，数据库提交后才应答渠道成功。条件须核对已存渠道、金额、币种和交易号，重复已支付通知或晚到 pending 不重复履约，已退款状态也不能倒退。订单号不得删除后复用，切换商户不能复用旧业务订单命名空间。外部发货等由 outbox 消费者携带稳定 outbox ID 幂等调用，不能在数据库事务中直接调用后宣称跨系统只执行一次。

### 5. 查询订单

Stripe 创建操作将 orderNo 与 Session ID 持久绑定，查单/退款通过该 ID 读取并核对订单号、金额和币种，不使用 metadata 列表首项猜测。metadata.orderNo 为保留字段。orderNo/refundNo 的请求参数不能在重试时改变，渠道 POST 使用稳定 Idempotency-Key。

Stripe closeOrder 会通过可信映射读取 Session，只有 open/unpaid 才调用 expire；已 expired 幂等成功，complete/paid 失败。并发关闭或响应丢失后必须重读确认 expired，不能把“已发送请求”当关闭成功。successUrl/cancelUrl 的页面跳转不代表已付款或已经关单，业务仍以可信状态为准。

响应丢失或落库失败会保留未知记录，只能在数据库时钟的 23 小时安全窗口内沿用同一键重试；窗口之外先核对渠道结果并修复记录，不删除后重发。成功结果长期保留，不按缓存 TTL 清理。Stripe 仅保存渠道操作结果，宿主业务订单、回调去重与履约事务仍需自己实现。

```typescript
const result = await payment.queryOrder('wechat', 'ORDER_20250101_001')

if (result.success) {
  // result.data.status: 'pending' | 'paid' | 'closed' | 'refunded' | 'failed'
}
```

### 6. 退款

支付宝查单/退款/关单会验证方法对应的 envelope、字符串 code=10000、关键字段和订单号；HTTP 200 业务错误返回对应操作的 HaiResult 失败。错误 cause 不含原始响应或 msg/sub_msg。退款 `success: true` 表示请求受理，仍须检查 data.status：fund_change=Y 才为 success，N/缺失为 processing，需自行通过渠道退款查询确认（本模块没有退款查询 API）。refund_fee 是累计退款额，refundId 是原交易号，以 refundNo 区分本次退款且重试保持参数不变。关单先检查 result.success 再更新业务订单。

```typescript
const result = await payment.refund('wechat', {
  orderNo: 'ORDER_20250101_001',
  refundNo: 'REFUND_20250101_001',
  amount: 9900,
  totalAmount: 9900, // 微信退款必填
  reason: '用户退款',
})
```

### 7. 客户端调起支付（浏览器）

```typescript
import { invokePayment } from '@h-ai/payment/client'

// 前端拿到服务端返回的 PaymentOrder 后调起支付
if (!orderResult.success)
  throw new Error(orderResult.error.message)
const payResult = await invokePayment(orderResult.data)
if (!payResult.success)
  throw new Error(payResult.error.message)
if (!payResult.data.invoked)
  throw new Error(payResult.data.message)
```

## 核心 API（服务端）

| API                                           | 用途                | 返回值                           |
| --------------------------------------------- | ------------------- | -------------------------------- |
| `payment.init(config)`                        | 初始化              | `HaiResult<void>`                |
| `payment.close()`                             | 关闭                | `void`                           |
| `payment.createOrder(providerName, input)`    | 创建订单            | `HaiResult<PaymentOrder>`        |
| `payment.handleNotify(providerName, request)` | 处理回调            | `HaiResult<PaymentNotifyResult>` |
| `payment.queryOrder(providerName, orderNo)`   | 查询订单            | `HaiResult<OrderStatus>`         |
| `payment.refund(providerName, input)`         | 退款                | `HaiResult<RefundResult>`        |
| `payment.closeOrder(providerName, orderNo)`   | 关闭订单            | `HaiResult<void>`                |
| `payment.getProvider(name)`                   | 获取 Provider       | `PaymentProvider \| undefined`   |
| `payment.registerProvider(provider)`          | 注册自定义 Provider | `void`                           |

## 客户端 API

| API                      | 用途     | 返回值                           |
| ------------------------ | -------- | -------------------------------- |
| `invokePayment(options)` | 调起支付 | `HaiResult<InvokePaymentResult>` |

## 错误码 — `HaiPaymentError`

| 错误码                                   | code              | 说明                      |
| ---------------------------------------- | ----------------- | ------------------------- |
| `HaiPaymentError.CREATE_ORDER_FAILED`    | `hai:payment:001` | 创建订单失败              |
| `HaiPaymentError.QUERY_ORDER_FAILED`     | `hai:payment:002` | 查询订单失败              |
| `HaiPaymentError.REFUND_FAILED`          | `hai:payment:003` | 退款失败                  |
| `HaiPaymentError.CLOSE_ORDER_FAILED`     | `hai:payment:004` | 关闭订单失败              |
| `HaiPaymentError.NOT_INITIALIZED`        | `hai:payment:010` | 模块未初始化              |
| `HaiPaymentError.SIGN_FAILED`            | `hai:payment:020` | 签名失败                  |
| `HaiPaymentError.PROVIDER_NOT_FOUND`     | `hai:payment:030` | Provider 未注册           |
| `HaiPaymentError.INVALID_AMOUNT`         | `hai:payment:040` | 金额无效                  |
| `HaiPaymentError.UNSUPPORTED_TRADE_TYPE` | `hai:payment:041` | 渠道不支持该模式          |
| `HaiPaymentError.INVALID_INPUT`          | `hai:payment:042` | openid/币种等下单参数无效 |
| `HaiPaymentError.NOTIFY_VERIFY_FAILED`   | `hai:payment:050` | 回调验签失败              |
| `HaiPaymentError.NOTIFY_PARSE_FAILED`    | `hai:payment:051` | 回调解析失败              |
| `HaiPaymentError.INVOKE_WEB_FAILED`      | `hai:payment:060` | Web 端调起支付失败        |
| `HaiPaymentError.CONFIG_ERROR`           | `hai:payment:070` | 配置无效                  |

## Provider 模式

内置 Provider：

- **wechat** — 微信支付 API v3
- **alipay** — 支付宝开放平台
- **stripe** — Stripe Checkout Session 模式

自定义 Provider 通过 `payment.registerProvider(provider)` 注册。

### HTTP API 契约

```typescript
import { apiContract } from '@h-ai/api-contract'

// apiContract.payment.orders.create        — POST /payment/orders
// apiContract.payment.orders.get           — GET  /payment/orders/{orderNo}
// apiContract.payment.orders.refund        — POST /payment/orders/{orderNo}/refunds
// apiContract.payment.notifications.wechat — POST /payment/notifications/wechat
```

## 常见模式

### 服务端路由（SvelteKit）

```typescript
import { kit } from '@h-ai/kit'
// src/routes/api/v1/payment/create/+server.ts
import { payment } from '@h-ai/payment'

export const POST = kit.handler(async ({ request, locals }) => {
  const body = await request.json()
  const result = await payment.createOrder(body.provider, {
    orderNo: body.orderNo,
    amount: body.amount,
    description: body.description,
    tradeType: body.tradeType,
    userId: body.userId,
    notifyUrl: body.notifyUrl,
  })

  if (!result.success) {
    return kit.response.internalError(result.error.message)
  }

  return kit.response.ok(result.data, locals.requestId)
})
```

### 完整支付流程

```
1. 客户端 → 服务端：创建订单（api.payment.orders.create({...})）
2. 服务端 → 支付平台：调用 Provider 创建预支付
3. 服务端 → 客户端：返回 providerData（含调起参数）
4. 客户端：invokePayment(providerData) 调起支付
5. 支付平台 → 服务端：异步回调通知
6. 服务端：handleNotify() 验签解析，业务层持久核对并幂等更新订单状态
7. 客户端：轮询或回调查询订单状态
```

## 审计日志

关键支付操作（创建订单、回调、退款、关闭订单）成功后自动写入审计日志（依赖 `@h-ai/audit`），无需额外配置。`queryOrder` 为只读操作，不写审计日志。

内置 Provider 的 HTTP 请求默认带 15 秒超时；Stripe 可通过 `webhookToleranceSeconds` 调整 webhook 时间戳容忍窗口。

## 相关 Skills

- `hai-kit`：服务端 API 路由与契约处理
- `hai-api-client`：客户端契约调用
- `hai-reldb`：订单持久化存储
- `hai-audit`：审计日志（payment 内部自动调用）
- `hai-core`：HaiResult 类型、日志、配置
