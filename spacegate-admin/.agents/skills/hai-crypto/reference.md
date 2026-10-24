# hai-crypto 参考

> `@h-ai/crypto` 提供非对称加密、哈希、对称加密与密码哈希能力，支持 Node.js 与浏览器双端。


认证协议仅支持 v2：独立随机 SM4-CBC 密钥和 HMAC-SM3 密钥由 SM2 封装，先验签、再验证 MAC，最后解密 CBC。请求由客户端私钥签名，响应由所选服务端版本签名。认证字段包括 version、direction、serverKeyId、clientId、128 位随机 requestId（兼 nonce）、issuedAt、HTTP method、pathname + 原始 search 的 SM3 pathHash，以及响应 status。请求有效期 5 分钟，未来时钟偏差最多 30 秒；共享 registry 原子消费 (clientId, requestId)，nonce 至少保留到 issuedAt + 6 分钟以覆盖节点偏差，存储故障拒绝请求。Redis 必须保留有效 nonce，使用 noeviction/可靠持久化策略；各节点时钟须同步。此能力防止同一传输消息重放，业务重试仍需自己的幂等键。

空 POST 也发送空明文的认证封装；GET/HEAD 将封装放在 X-Hai-Transport 头，204/304/HEAD 等无正文响应也使用同名头承载认证封装。其它请求/响应在 JSON 正文承载封装并带 X-Encrypted: true。客户端必须关联原请求上下文和 HTTP status，缺标记、旧格式或认证失败一律 reject；明文接口只能经 excludePaths 显式排除。CORS 须允许并暴露 X-Hai-Transport。握手公钥的真实性依赖 TLS，不替代 IAM 业务授权。

## 运行环境

**Node.js + 浏览器双端可用。** 浏览器端通过 `crypto.init()` 初始化（无需配置参数）。

| 能力 | Node.js | 浏览器 | 说明 |
|------|---------|--------|------|
| `crypto.asymmetric` 非对称加密 | ✅ | ✅ | SM2 加解密、签名验签 |
| `crypto.hash` 哈希 | ✅ | ✅ | SM3 哈希、HMAC |
| `crypto.symmetric` 对称加密 | ✅ | ✅ | SM4 ECB/CBC |
| `crypto.password` 密码哈希 | ✅ | ❌ | 依赖 Node.js crypto（服务端专用） |
| `crypto.transport` 传输加密 | ✅ | ✅ | 服务端管理器 + 客户端 encryptedFetch |

浏览器端主要用于 kit 传输加密场景（`kit.client.create({ transport: { crypto } })`），一般不需要直接调用 crypto API。

## 适用场景

- 非对称加密/解密、签名/验签、密钥生成
- 哈希计算与 HMAC
- 对称加密/解密（ECB/CBC 模式）
- 密码存储与验证（加盐迭代哈希）
- 传输加密：为 serv / kit / api-client 提供统一协议、密钥协商和请求响应加解密

## 使用步骤

### 1. 初始化与关闭

```typescript
import { crypto } from '@h-ai/crypto'

await crypto.init()

// 使用后关闭
await crypto.close()
```

## 核心 API

### 非对称加密 — `crypto.asymmetric`

| 方法                | 签名                                                        | 说明         |
| ------------------- | ----------------------------------------------------------- | ------------ |
| `generateKeyPair`   | `() => HaiResult<KeyPair>`                                     | 生成密钥对   |
| `encrypt`           | `(data, publicKey, options?) => HaiResult<string>`             | 公钥加密     |
| `decrypt`           | `(ciphertext, privateKey, options?) => HaiResult<string>`      | 私钥解密     |
| `sign`              | `(data, privateKey, options?) => HaiResult<string>`            | 私钥签名     |
| `verify`            | `(data, signature, publicKey, options?) => HaiResult<boolean>` | 公钥验签     |
| `isValidPublicKey`  | `(key: string) => boolean`                                  | 校验公钥编码和 SM2 曲线点 |
| `isValidPrivateKey` | `(key: string) => boolean`                                  | 校验私钥格式 |

```typescript
const keyPair = crypto.asymmetric.generateKeyPair()
if (keyPair.success) {
  const { publicKey, privateKey } = keyPair.data

  const encrypted = crypto.asymmetric.encrypt('敏感数据', publicKey)
  if (encrypted.success) {
    const decrypted = crypto.asymmetric.decrypt(encrypted.data, privateKey)
  }

  const signature = crypto.asymmetric.sign('待签名数据', privateKey)
  if (signature.success) {
    const valid = crypto.asymmetric.verify('待签名数据', signature.data, publicKey)
  }
}
```

**AsymmetricEncryptOptions**：`{ cipherMode?: 0 | 1, outputFormat?: 'hex' | 'base64' }`
**SignOptions**：`{ hash?: boolean, userId?: string }`

### 哈希 — `crypto.hash`

| 方法     | 签名                                                       | 说明     |
| -------- | ---------------------------------------------------------- | -------- |
| `hash`   | `(data: string \| Uint8Array, options?) => HaiResult<string>` | 哈希     |
| `hmac`   | `(data, key) => HaiResult<string>`                            | HMAC     |
| `verify` | `(data, expectedHash) => HaiResult<boolean>`                  | 验证哈希 |

```typescript
const hash = crypto.hash.hash('hello')
const hmac = crypto.hash.hmac('hello', 'secret-key')
const valid = crypto.hash.verify('hello', hash.data!)
```

**HashOptions**：`{ inputEncoding?: 'utf8' | 'hex' }`

### 对称加密 — `crypto.symmetric`

| 方法            | 签名                                            | 说明                        |
| --------------- | ----------------------------------------------- | --------------------------- |
| `generateKey`   | `() => string`                                  | 生成 128-bit 密钥（32 hex） |
| `generateIV`    | `() => string`                                  | 生成随机 IV（32 hex）       |
| `encrypt`       | `(data, key, options?) => HaiResult<SymmetricEncryptedPayload>` | 加密（返回结构化密文）      |
| `decrypt`       | `(payload, key) => HaiResult<string>`          | 解密结构化密文              |
| `encryptWithIV` | `(data, key) => HaiResult<EncryptWithIVResult>`    | CBC 模式加密（自动生成 IV） |
| `decryptWithIV` | `(ciphertext, key, iv) => HaiResult<string>`       | CBC 模式解密                |
| `isValidKey`    | `(key: string) => boolean`                      | 校验密钥格式                |
| `isValidIV`     | `(iv: string) => boolean`                       | 校验 IV 格式                |

SM4 密钥使用 `generateKey()` 生成；本模块不提供密码 KDF。密码存储使用服务端 `crypto.password.hash()`，不要将密码或密码哈希直接用作 SM4 密钥。

```typescript
const key = crypto.symmetric.generateKey()

// 默认安全用法：CBC + 自动 IV，返回 { mode, ciphertext, iv, encoding }
const encrypted = crypto.symmetric.encrypt('明文数据', key)
if (encrypted.success) {
  const decrypted = crypto.symmetric.decrypt(encrypted.data, key)
}

// 需要单独存储 IV 时使用结构化结果
const withIV = crypto.symmetric.encryptWithIV('明文数据', key)
if (withIV.success) {
  const decrypted = crypto.symmetric.decryptWithIV(withIV.data.ciphertext, key, withIV.data.iv)
}
```

**SymmetricEncryptOptions**：`{ mode?: 'ecb' | 'cbc', iv?: string, outputFormat?: 'hex' | 'base64' }`
**SymmetricEncryptedPayload**：`{ mode: 'ecb' | 'cbc', ciphertext: string, iv?: string, encoding: 'hex' | 'base64' }`

> `encrypt()` 默认使用 CBC 并自动生成 IV。ECB 会泄漏明文结构，只有底层协议明确要求时才显式 `{ mode: 'ecb' }`。

### 密码哈希 — `crypto.password`

| 方法     | 签名                                    | 说明     |
| -------- | --------------------------------------- | -------- |
| `hash`   | `(password, config?) => HaiResult<string>` | 密码哈希 |
| `verify` | `(password, hash) => HaiResult<boolean>`   | 验证密码 |

```typescript
const hashed = crypto.password.hash('MyPassword123')
if (hashed.success) {
  const valid = crypto.password.verify('MyPassword123', hashed.data)
}
```

哈希格式：`$hai$<iterations>$<salt>$<hash>`
**PasswordConfig**：`{ saltLength?: number, iterations?: number }`

### 传输加密 — `crypto.transport`

传输加密主工厂必须从 `crypto.transport` 访问；共享 `keyStore` provider 请从 `@h-ai/crypto` 根入口导入，不从子目录内部路径导入。

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `createServer` | `({ serverKeyRing, keyStore }) => HaiResult<TransportEncryptionManager>` | 校验受管密钥环并创建管理器，两项必填 |
| `createClient` | `({ keyExchangeUrl, fetch? }) => TransportClient` | 创建客户端 encryptedFetch 会话 |
| `protocol` | `TRANSPORT_PROTOCOL` | 协议常量：`X-Client-Id`、`X-Encrypted`、默认协商路径 |

共享 `keyStore` provider（从 `@h-ai/crypto` 根入口导入）：

| 工厂 | 签名 | 说明 |
| --- | --- | --- |
| `createInMemoryKeyStore` | `(maxClients?) => TransportKeyStore` | 默认 FIFO 内存实现 |
| `createRedisTransportKeyStore` | `({ cache, ttlSeconds? }) => TransportKeyStore` | 基于 `@h-ai/cache` 的共享实现，通常配 Redis |
| `createReldbTransportKeyStore` | `({ reldb, ttlSeconds?, maxClients? }) => ReldbTransportKeyStore` | 自动建表、共享事务容量和有界维护 |

所有 registry 注册前验证非压缩 SM2 曲线点（128 hex 或 04 + 128 hex）；纯函数 `isValidSM2PublicKey(value)` 无需 init，serv/kit 在写存储前以 400 拒绝畸形输入。Redis/DB TTL 省略则不过期，显式值须为 1–2147483647 整数秒，建议有限 TTL。Redis 用原生逐键 TTL，容量由部署管理；noeviction 防止提前驱逐 nonce。DB `maxClients` 默认 10000，正安全整数且各节点须与持久容量行一致；事务锁容量行后计数和插入，容量满拒绝、不驱逐有效公钥或 nonce。

DB 首次建表失败可重试；公钥过期使用 DB epoch 毫秒，DB 与节点均须同步时钟。注册自动执行一批清理，应用定期调用 `await registry.pruneExpired()` 处理低流量维护：每次最多 100 条过期公钥和 100 条过期 nonce，不建隐式计时器。旧 timestamp 表须停机迁移到 created_at_ms/expires_at_ms BIGINT；容量更改需同步持久记录和所有节点，不能清空有效 nonce。

```ts
const registry = createReldbTransportKeyStore({ reldb, ttlSeconds: 3600, maxClients: 10000 })
// 由应用自己的维护调度器调用；失败后可重试。
await registry.pruneExpired()
```

```typescript
const server = crypto.transport.createServer({ serverKeyRing, keyStore })
if (!server.success)
  return server

const client = crypto.transport.createClient({
  keyExchangeUrl: 'https://api.example.com/api/v1/_hai/key-exchange',
})

const resp = await client.encryptedFetch('https://api.example.com/api/v1/echo', {
  method: 'POST',
  body: JSON.stringify({ hello: 'world' }),
})
```

```typescript
import { createRedisTransportKeyStore, crypto } from '@h-ai/crypto'
import { cache } from '@h-ai/cache'

await cache.init({ type: 'redis', host: '127.0.0.1', port: 6379 })

const sharedServer = crypto.transport.createServer({
  serverKeyRing,
  keyStore: createRedisTransportKeyStore({ cache, ttlSeconds: 3600 }),
})
```

使用流程：
1. 先 `await crypto.init()`。
2. 服务端显式传入受管 serverKeyRing 和共享 keyStore，协商返回 `serverKeyId + serverPublicKey + clientId`。
3. 客户端 `createClient({ keyExchangeUrl })` 后，首次 `init()` / `encryptedFetch()` 会自动做密钥协商。
4. 所有请求均发送 v2 认证封装；包括 GET/HEAD 和无 input POST，不能只附带 clientId。
5. 自定义适配器先 await authenticateTransportRequest(manager, request)，再用 manager.encryptResponse(verified.data.context, data, status) 关联响应。
6. 客户端收到 `X-Encrypted: true` 的响应后自动解密；`destroy()` 可清空当前会话。

`encryptedFetch()` 与原生 fetch 一样会 reject：网络错误、密钥协商失败、请求加密失败或响应解密失败都应在调用方按 fetch 错误处理策略捕获。

### 受管服务端密钥与轮换

`serverKeyRing` 为 `{ activeKeyId, keys: { [id]: { publicKey, privateKey } } }`，ID 为 1–64 个字母、数字、下划线或连字符。根入口 `parseTransportServerKeyRing(secret)` 接受服务端 Secret JSON 或配置对象，返回 HaiResult，错误不附带原始 Secret 或解析片段。工厂验证每对公私钥并捕获快照；没有隐式随机私钥或 memory registry 回退。`keyStore` 也必填，生产用共享 Redis/数据库；开发/测试可显式 `createInMemoryKeyStore(capacity)`。

示例服务端读取 `process.env.HAI_CRYPTO_TRANSPORT_SERVERKEYRING`，检查解析结果后仅传给服务端工厂；禁止写入普通 YAML、PUBLIC_/VITE_、日志或响应。CLI 仅在创建服务端项目时向忽略的 `.env.local` 生成一次本机开发 ring，生产由 Secret Manager 分发同一受管 ring。

请求密文 `EncryptedRequestPayload` 携带协商的 `serverKeyId`；服务端只按 ID 选择私钥，未知/撤销 ID 拒绝，不尝试其它 key。轮换先向所有节点分发包含新旧版本的 ring，再切 activeKeyId 并滚动重建 manager；过渡期后删除旧 ID 并更新所有节点。旧客户端 destroy 后重协商；禁止自动重放非幂等请求。共享 ring + registry 支持跨节点与重启，不依赖 sticky session。

```ts
import { parseTransportServerKeyRing } from '@h-ai/crypto'

const ring = parseTransportServerKeyRing(process.env.HAI_CRYPTO_TRANSPORT_SERVERKEYRING)
if (!ring.success)
  throw new Error(ring.error.message)
const manager = crypto.transport.createServer({ serverKeyRing: ring.data, keyStore: sharedRegistry })
```

### 传输正文预算与自定义适配器

serv、kit 和客户端统一从根入口导入 `TRANSPORT_BODY_LIMITS`：握手 JSON 16 KiB、单个公钥 4 KiB、明文 1 MiB、密文 JSON 2 MiB + 32 KiB，均为实际 UTF-8 字节上限，不使用 Content-Length 判定。客户端共享握手最多等待 30 秒；单个业务请求取消只中断自己的等待，其 signal 继续用于请求及响应读取。异常加密响应会 reject，不返回原密文。

根入口纯函数 `readTransportBody(stream, { maxBytes, signal? })` 无需 init，直接消费原始流；超限抛 RangeError，无效 UTF-8 抛 TypeError，保留读取异常或 signal.reason。超限/取消会立即取消流并释放 reader，清理失败不覆盖原异常。不要把 clone/tee 分支交给它，否则另一分支仍可能缓冲正文。成功读取后重建 Request/Response，清除旧 Content-Length/Content-Encoding 并保留 signal。

```ts
import { readTransportBody, TRANSPORT_BODY_LIMITS } from '@h-ai/crypto'

const text = await readTransportBody(request.body, {
  maxBytes: TRANSPORT_BODY_LIMITS.keyExchangeBytes,
  signal: request.signal,
})
const payload: unknown = JSON.parse(text)
```

常规应用优先使用上层封装：
- serv：`serv.createApp({ transport: { crypto, serverKeyRing, keyStore } })`
- kit：`kit.createHandle({ crypto: { crypto, transport: { serverKeyRing, keyStore } } })` + `kit.client.create({ transport: { crypto } })`
- api-client：`apiClient.init({ transport: { crypto } })`

## 错误码 — `HaiCryptoError`

| 错误码 | code | 说明 |
|--------|------|------|
| `HaiCryptoError.INVALID_INPUT` | `hai:crypto:002` | 无效输入 |
| `HaiCryptoError.INVALID_KEY` | `hai:crypto:003` | 无效密钥 |
| `HaiCryptoError.NOT_INITIALIZED` | `hai:crypto:010` | 未初始化 |
| `HaiCryptoError.INIT_FAILED` | `hai:crypto:011` | 初始化失败 |
| `HaiCryptoError.KEY_GENERATION_FAILED` | `hai:crypto:020` | 密钥生成失败 |
| `HaiCryptoError.ENCRYPTION_FAILED` | `hai:crypto:021` | 加密失败 |
| `HaiCryptoError.DECRYPTION_FAILED` | `hai:crypto:022` | 解密失败 |
| `HaiCryptoError.SIGN_FAILED` | `hai:crypto:023` | 签名失败 |
| `HaiCryptoError.VERIFY_FAILED` | `hai:crypto:024` | 验签失败 |
| `HaiCryptoError.HASH_FAILED` | `hai:crypto:040` | 哈希计算失败 |
| `HaiCryptoError.HMAC_FAILED` | `hai:crypto:041` | HMAC 计算失败 |
| `HaiCryptoError.INVALID_IV` | `hai:crypto:060` | 无效 IV |

## 常见模式

### 数据加密存储

```typescript
const key = process.env.HAI_CRYPTO_DATA_KEY!
const encrypted = crypto.symmetric.encrypt(sensitiveData, key)
// 存入数据库

// 读取时解密
const decrypted = crypto.symmetric.decrypt(storedPayload, key)
```

> 默认密文是结构化对象，必须完整存储 `mode` / `ciphertext` / `iv` / `encoding` 字段；如果数据库字段已拆分 IV 与密文，请改用 `encryptWithIV()` / `decryptWithIV()`。

### 密码存储与验证

```typescript
// 注册时哈希存储
const hashResult = crypto.password.hash(userPassword)
if (hashResult.success) {
  // 存储 hashResult.data 到数据库
}

// 登录时验证
const verifyResult = crypto.password.verify(inputPassword, storedHash)
if (verifyResult.success && verifyResult.data) {
  // 密码匹配
}
```

## 相关 Skills

- `hai-core`：配置与 HaiResult 模型
- `hai-iam`：密码哈希（内部自动调用 crypto.password）
- `hai-kit`：SvelteKit 集成（传输加密）
- `hai-serv` / `hai-api-client`：跨域 HTTP API 的透明传输加密

管理端在线加密演示直接调用 Crypto SM3/SM4 API，SM4 使用随机临时密钥和 IV，并在显示成功前解密校验；密钥不保存。输入改变即清空旧结果，禁止用随机摘要或 Base64 伪装算法结果。

## 生产共享状态

生产传输 registry 必须声明 stateScope=shared 并实际使用 Redis 或 PostgreSQL/MySQL。memory、包装 memory 的 Redis 工厂、包装 SQLite 的 RelDB 工厂会被拒绝；各节点加载相同受管版本化密钥环。 NODE_ENV 或 HAI_CORE_ENV 为 production 时在初始化成功前检查，只有 HAI_E2E=1/HAI_E2E_MOCK=1 明确测试豁免。不要将租约当作最终写入条件，外部未知效果须按稳定业务键核对。
