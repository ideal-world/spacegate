/**
 * 生成本地/预发使用的 SM2 传输密钥环。
 *
 * 生产环境必须由 Secret Manager 向所有服务节点分发同一份受管 ring，
 * 不要把本脚本输出提交到 Git、镜像或日志。轮换时先分发包含新旧 key 的 ring，
 * 再切换 activeKeyId，最后移除旧 key。
 *
 * @example
 * ```sh
 * node deploy/generate-transport-keyring.mjs
 * ```
 */
import { createECDH } from 'node:crypto'

const key = createECDH('SM2')
key.generateKeys()
const ring = {
  activeKeyId: 'local-v1',
  keys: {
    'local-v1': {
      publicKey: key.getPublicKey('hex'),
      privateKey: key.getPrivateKey('hex').padStart(64, '0'),
    },
  },
}
process.stdout.write(`${JSON.stringify(ring)}\n`)
