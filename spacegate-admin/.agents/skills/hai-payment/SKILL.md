---
name: hai-payment
description: "接入微信、支付宝或 Stripe 的订单、回调验签、查单与退款，并处理客户端调起和幂等履约。"
---

# hai-payment

## 能力契约

| 项目     | 契约                                                                                        |
| -------- | ------------------------------------------------------------------------------------------- |
| 能力     | 使用 @h-ai/payment 创建支付订单、验证回调、查单和退款，客户端调起支付                       |
| 适用场景 | 服务端支付集成及客户端调起流程                                                              |
| 输入     | Provider 配置、订单/退款参数、原始回调内容                                                  |
| 输出     | 服务端 HaiResult；客户端 invokePayment 按 client 契约使用                                   |
| 限制     | 服务端保存凭据并验签；客户端完成不代表付款成功，以可信回调/查单为准。订单履约与重试需幂等。 |

## 使用路径

1. 服务端校验 Provider 配置后 init；微信必填 platformKeys（平台 serial/公钥ID→PEM，轮换保留新旧项），支付宝必填 sellerId 和可信 HTTPS returnUrl。Stripe 先初始化共享 reldb，记录渠道映射/幂等结果，关闭时顺序相反。真实凭据经私有配置注入；浏览器只导入 `/client`。
2. 订单号、金额单位、商品与用户由服务端可信数据决定，先保存业务订单再创建支付。微信仅 jsapi，必填当前公众号 openid，微信/支付宝仅 CNY；支付宝仅 h5，用离线签名的 `{ gateway, fields }` 向固定网关 POST，成功仅表示生成页面参数。Stripe 仅 h5 托管 Checkout，配置 successUrl（必填）/cancelUrl（可选）。公共 TradeType 仅 jsapi/h5；App、Native、小程序及其它组合在联网前拒绝，客户端也拒绝。客户端先检查 success 再检查 invoked，不解析不可信 HTML，不接受任意 action/字段。
3. 回调使用原始 body 与 headers 验签；核对商户/应用身份与严格时间格式，微信签名时钟仅容差 300 秒，支付宝允许合法跨日重试。所有 Provider 返回必填 eventId/currency，应用事务核对渠道、订单、金额、币种，以共享数据库原子提交订单状态/通知去重/outbox，提交后才应答成功；外部副作用需稳定键幂等。Stripe completed/unpaid 为 pending，只有 status=paid 才履约且防止乱序回退。客户端调起成功不代表付款完成。
4. 查单、退款与回调重试覆盖成功、失败和重复路径；支付宝查单/退款/关单的 HTTP 200 不代表业务成功，缺失/错误 envelope、非 10000 业务码或关键字段缺失返回对应操作失败；退款 success 后还须检查 status，fund_change=N/缺失为 processing，向渠道确认退款结果。Stripe orderNo/refundNo 固定请求参数，不能覆盖 metadata.orderNo，未知结果超过 23 小时先核对渠道，不删除记录后盲重试。关单须确认 Session expired，已付款不能关闭；页面跳转不能作履约凭据。真实支付仅在用户授权范围内，最后 close。

配置示例区分 `PaymentConfigInput` 与 Schema 解析后的 `PaymentConfig`；公开调用只使用包根入口或浏览器 `/client`。先判断 `HaiResult.success`，再判断付款/退款状态或客户端 `invoked`。源码内部签名/协议/幂等仓库例子仅用于解释实现，不应深层导入；类型示例不证明真实商户、网关或手机设备支付。

## 按需参考

按本次操作读取对应小节；以已安装版本的公开类型核验签名，示例中的业务变量由应用提供。

- [运行环境](reference.md#运行环境)
- [依赖](reference.md#依赖)
- [适用场景](reference.md#适用场景)
- [使用步骤](reference.md#使用步骤)
- [核心 API（服务端）](reference.md#核心-api服务端)
- [客户端 API](reference.md#客户端-api)
- [错误码 — `HaiPaymentError`](reference.md#错误码--haipaymenterror)
- [Provider 模式](reference.md#provider-模式)
- [常见模式](reference.md#常见模式)
- [审计日志](reference.md#审计日志)
- [相关 Skills](reference.md#相关-skills)
