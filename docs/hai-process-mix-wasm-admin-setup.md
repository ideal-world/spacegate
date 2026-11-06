# 在 Spacegate 管理后台配置 hai-process-mix Wasm 插件

本文记录如何把 `hai-process-mix/wasm-server/plugin.wasm` 作为自定义 Wasm 插件加入管理后台，并绑定到路由后实际验证。

> 说明：`hai-process-mix` 是旧版 Wasm 实现，依赖外部 `ac-service` 和 `asset-service`。
> 当前仓库推荐的 HAI 原生插件链（`hai-auth → hai-asset → hai-quota → hai-dispatch`）使用 Redis，二者不要混用同一套测试数据。
> 本文只解决“这个 Wasm 制品如何被 Spacegate 管理后台加载、配置和绑定”的问题。

## 新管理后台（spacegate-admin）页面操作

新后台是 `spacegate-admin`（Svelte），不是旧的 `spacegate-admin-fe`（Vue）。
页面路径为 `插件 → Wasm 扩展`，路由绑定在 `路由 → 编辑 → 插件链` 中完成。

### 1. 进入页面并选择网关

1. 登录新后台。
2. 在顶部“选择网关”下拉框中选择目标网关，例如本地 all-in-one 的 `spacegate-admin`。
3. 左侧菜单进入 `插件`，切换到 `Wasm 扩展` 页签。

顶部没有选中网关时，Wasm 卡片上的“启用/停用”按钮不可用，但创建插件定义本身不受影响。

### 2. 添加自定义插件定义

点击 `添加自定义插件`，按以下页签填写。

#### 插件定义

| 页面字段 | 填写值 |
| --- | --- |
| 插件名称 | `hai-process-mix` |
| 展示名称 | `hai-process-mix` |
| 描述 | `API key auth, asset routing, rate limiting and request dispatch plugin.` |
| 镜像地址 | `file:///plugins/hai-process-mix.wasm` |
| schema 路径 | `hai-process-mix.schema.json` |
| 运行时插件名称 | `hai-process-mix` |
| 生效阶段 | `认证` |
| 优先级 | `100` |
| 镜像拉取策略 | `不存在时拉取` |
| 失败策略 | `失败时放行` |
| 普通 VM 池大小 | `1` |
| Wait VM 池大小 | `0` |
| 复用模块缓存 | 打开 |

说明：

- “插件名称”只支持小写字母、数字和中划线，创建后不可修改。
- “镜像地址”填的是网关容器内可读的路径；`file:///plugins/...` 不是宿主机路径。
- 如果 admin-server 读不到该路径，`schema 路径` 将无法预览；仍可保存定义，但绑定配置时只能使用 JSON/YAML 模式。

#### 默认配置

把下面这段 JSON 粘贴到“默认配置”文本框：

```json
{
  "ac_service_host": "host.docker.internal",
  "ac_service_port": 18081,
  "asset_service_host": "host.docker.internal",
  "asset_service_port": 18082,
  "ac_auth_path": "/ai-agent/internal/v1/ac/auth",
  "asset_lookup_path": "/ai-agent/internal/v1/am/assets/{asset_id}",
  "report_service_host": "",
  "report_service_port": 80,
  "report_path": "/internal/v1/llm-usage",
  "report_timeout_ms": 1000,
  "gateway_hosts": [],
  "output_guard_enabled": true,
  "allowed_output_targets": [],
  "model_error_keywords": ["error", "ERR_", "FAILED"]
}
```

“禁用默认配置”保持关闭。保存后新后台会把它同时写入 `default_config` 和运行时 `plugin_config`。

#### 匹配规则

保持空数组：

```json
[]
```

#### 高级参数

`Clusters` 粘贴：

```json
{
  "outbound|18081||host.docker.internal": "http://host.docker.internal:18081",
  "outbound|18082||host.docker.internal": "http://host.docker.internal:18082"
}
```

`资源限制` 保持：

```json
{}
```

`Plugin Root ID`、`Plugin VM ID` 不填时会自动生成；模块缓存键可留空。

#### OCI 认证 / HTTP(S) 认证

本地 `file://` 场景不需要填写，直接留空。

点击 `保存` 后，`Wasm 扩展` 页会出现 `hai-process-mix` 卡片，并提示：

```text
Wasm 插件配置已更新，请在实例运维中执行全局重载后生效。
```

### 3. 让插件生效

新后台有两种绑定方式，按测试范围选择。

#### 方式 A：网关级启用（最简单）

在 `Wasm 扩展` 卡片上点击 `启用`。

这会把这个 Wasm 定义直接加入当前网关的插件绑定，对所有经过该网关的请求生效。卡片状态会变成 `当前网关已启用`。

#### 方式 B：路由级绑定（推荐测试）

1. 左侧菜单进入 `路由`。
2. 顶部选择目标网关。
3. 编辑目标路由，或者新建一条测试路由。
4. 切换到 `插件链` 页签。
5. 在“路由级插件”区域点击 `绑定插件`。
6. 在弹窗中切换到 `Wasm 扩展` 页签。
7. 选择 `hai-process-mix`。
8. 新后台会自动打开 `配置 Wasm 绑定`：
   - 配置方式选择 `默认配置`，会直接复用插件定义中的 `plugin_config`；
   - 也可以选择 `JSON`，把上面的默认配置再粘贴一遍，作为该路由的独立配置。
9. 保存绑定。路由插件链中会出现类似 `wasm / bind-xxxxxxxx` 的绑定实例，这是正常的：
   - `bind-xxxxxxxx` 是新后台自动创建的绑定配置；
   - 它的显示名类似 `route / <路由名>-route / hai-process-mix`；
   - 基础定义仍然是 `wasm / hai-process-mix`。
10. 回到路由编辑器，点击 `保存`，把路由插件链写入 admin-server。

路由必须能匹配插件识别的路径：

```text
/api/v1/<asset_type>/<asset_id>
/mcp-services/<asset_id>
```

### 4. 执行全局重载

进入左侧菜单 `实例`：

1. 选择目标网关。
2. 找到当前实例。
3. 点击 `全局重载` 并确认。

重载成功后，网关日志应出现：

```text
[hai-process-mix] vm started
[hai-process-mix] configured: ac=... asset=...
wasm plugin: VM pools created pool_size=1 wait_pool_size=0
```

### 5. 页面操作后的验证

```bash
curl -i \
  -H 'Host: test.local' \
  -H 'Hai-Api-Key: demo-key' \
  http://127.0.0.1:9991/api/v1/skill/demo-skill
```

返回 `200` 且 body 为 `demo-skill` 的 `asset_content`，说明定义、绑定和重载都已生效。

## 1. 制品放置

网关和 admin-server 需要能读到同一个文件。all-in-one 镜像中外部 Wasm 目录是 `/plugins`：

```bash
docker cp /path/to/plugin.wasm spacegate-all-in-one:/plugins/hai-process-mix.wasm
docker cp /path/to/schema.json spacegate-all-in-one:/plugins/hai-process-mix.schema.json
```

本次使用的文件：

| 文件 | 说明 |
| --- | --- |
| `/plugins/hai-process-mix.wasm` | 插件二进制，sha256 `994c590637b963279b1de5d39fb78474c903e81738688816f94e20680a4180de` |
| `/plugins/hai-process-mix.schema.json` | 插件配置 schema，用于后台动态表单 |

`schema_path` 是相对于 Wasm 文件所在目录的路径。不要把多个插件的 schema 都命名为 `schema.json`，否则会互相覆盖。

## 2. 在管理后台创建自定义插件

入口：插件中心 → Wasm 扩展 → 添加自定义插件。

关键字段：

| 字段 | 值 |
| --- | --- |
| 插件名称 | `hai-process-mix` |
| 显示名称 | `hai-process-mix` |
| 镜像地址 | `file:///plugins/hai-process-mix.wasm` |
| Schema 文件路径 | `hai-process-mix.schema.json` |
| 生效阶段 | `AUTHN` |
| 优先级 | `100` |
| 失败策略 | `fail_open` |
| VM 池 | `1` |

也可以直接调用 admin-server API：

```bash
curl -X POST \
  'http://127.0.0.1:9992/config/plugin?code=wasm&kind=named&name=hai-process-mix&display_name=hai-process-mix' \
  -H 'Content-Type: application/json' \
  -H "X-Client-Version: <当前 X-Server-Version>" \
  --data-binary @hai-process-mix-plugin.json
```

保存后应能在以下接口看到定义：

```bash
curl -s http://127.0.0.1:9992/config/plugins/wasm
curl -s 'http://127.0.0.1:9992/plugin/wasm/schema?code=wasm&kind=named&name=hai-process-mix'
```

## 3. 运行时配置

插件的 `plugin_config` 会被 Spacegate 序列化成 YAML 后传给 guest 的 `proxy_on_configure`。示例：

```json
{
  "ac_service_host": "host.docker.internal",
  "ac_service_port": 18081,
  "asset_service_host": "host.docker.internal",
  "asset_service_port": 18082,
  "ac_auth_path": "/ai-agent/internal/v1/ac/auth",
  "asset_lookup_path": "/ai-agent/internal/v1/am/assets/{asset_id}",
  "report_service_host": "",
  "report_service_port": 80,
  "report_path": "/internal/v1/llm-usage",
  "report_timeout_ms": 1000,
  "gateway_hosts": [],
  "output_guard_enabled": true,
  "allowed_output_targets": [],
  "model_error_keywords": ["error", "ERR_", "FAILED"]
}
```

如果 AC/资产服务在 Kubernetes 中，把 host 换成实际 Service 名或 Higress cluster 名；端口按服务实际端口填写。

### 关于 clusters

插件会为 AC/资产服务构造 Higress 风格 cluster：

```text
outbound|<port>||<host>
```

Spacegate 的 Wasm host 会先精确匹配 `clusters` 映射；不命中时再按 `outbound|<port>||<host>` 回退成 `http://<host>:<port>`。显式配置更清晰：

```json
{
  "outbound|18081||host.docker.internal": "http://host.docker.internal:18081",
  "outbound|18082||host.docker.internal": "http://host.docker.internal:18082"
}
```

## 4. 绑定到路由

插件定义本身不会改变流量，需要在路由、规则或网关上引用。插件只处理以下路径：

```text
/api/v1/<asset_type>/<asset_id>
/mcp-services/<asset_id>
```

本地验证用路由示例：

```json
{
  "route_name": "hai-process-mix-test",
  "plugins": [
    { "code": "wasm", "kind": "named", "name": "hai-process-mix", "priority": 0 }
  ],
  "rules": [
    {
      "matches": [
        { "path": { "kind": "Prefix", "value": "/api/v1/" } }
      ],
      "backends": [
        {
          "host": { "kind": "Host", "host": "127.0.0.1" },
          "port": 80,
          "protocol": "http",
          "downgrade_http2": false,
          "weight": 1
        }
      ]
    }
  ],
  "priority": 100
}
```

绑定后必须执行全局重载：

```bash
curl -X POST \
  'http://127.0.0.1:9992/discovery/instance/reload/global?instance=local' \
  -H "X-Client-Version: <当前 X-Server-Version>"
```

网关日志出现以下内容即表示插件已加载并完成配置：

```text
[hai-process-mix] vm started
[hai-process-mix] configured: ac=... asset=...
wasm plugin: VM pools created pool_size=1 wait_pool_size=0
```

## 5. 验证

`skill` 类型资产会由插件直接返回 `asset_content`，不依赖最终上游：

```bash
curl -i \
  -H 'Host: test.local' \
  -H 'Hai-Api-Key: demo-key' \
  http://127.0.0.1:9991/api/v1/skill/demo-skill
```

本次实测结果：

- 正常请求返回 `200`，body 为 `demo-skill` 的 `asset_content`。
- 缺少 `Hai-Api-Key` 返回 `401 missing_api_key`。
- Key 未订阅资产返回 `403 not_subscribed`。

本地 AC/资产 mock 可用 `hai-process-mix/plugins/mix/deploy/mocks` 启动：

```bash
docker run -d --rm --name hai-mix-mock-ac \
  -p 18081:8080 \
  -v "$PWD/mock.py:/app/mock.py:ro" \
  -e MOCK_ROLE=ac -e PORT=8080 \
  python:3.12-alpine python /app/mock.py

docker run -d --rm --name hai-mix-mock-asset \
  -p 18082:8080 \
  -v "$PWD/mock.py:/app/mock.py:ro" \
  -e MOCK_ROLE=asset -e PORT=8080 \
  python:3.12-alpine python /app/mock.py
```

## 6. 生产 / K8s 注意事项

1. `file:///plugins/...` 是网关容器内路径。每个可能运行网关 Pod 的节点都必须有该文件；K8s 当前通常把节点 `/opt/spacegate/wasm` 挂载到 `/plugins`。
2. 新环境优先使用 OCI 或 HTTPS 地址，避免节点文件分发和版本漂移；远端更新时同时更新 `sha256` 或 `module_cache_key`。
3. admin-server 如果独立于网关 Pod，必须同样能读到 `file://` 路径才能预览 schema；否则 schema 预览为空，只能使用 JSON 模式，或改用 HTTP(S)/OCI。
4. 插件对 `model` / `api` 资产的最终转发依赖 `hai-gw-server` 读取 `Hai-*` 控制头。因此绑定该插件的路由后端应指向 `hai-gw-server`，不能直接指向最终业务上游；`skill` 等直接响应资产不受此限制。
5. 本地测试路由 `hai-process-mix-test` 的优先级为 `100`，接入真实业务前应调整或删除，避免覆盖已有 `/api/v1/` 路由。
6. 不再需要 mock 时：

```bash
docker rm -f hai-mix-mock-ac hai-mix-mock-asset
```
