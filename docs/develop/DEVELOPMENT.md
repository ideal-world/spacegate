# Spacegate 开发者指南

本文档面向希望参与 Spacegate 核心开发、贡献代码或开发自定义插件的开发者。

---

## 目录

1. [环境搭建与构建](#1-环境搭建与构建)
2. [项目架构与 Crate 职责](#2-项目架构与-crate-职责)
3. [请求生命周期](#3-请求生命周期)
4. [插件开发教程](#4-插件开发教程)
5. [配置系统](#5-配置系统)

---

## 1. 环境搭建与构建

### 1.1 前置依赖

| 依赖 | 版本要求 | 说明 |
|------|----------|------|
| Rust (stable) | ≥ 1.76 | `rustup update stable` |
| cargo-make | 最新版 | `cargo install cargo-make` |
| Docker | 任意版本 | 可选，构建 k8s 镜像时需要 |

### 1.2 克隆项目

```bash
git clone https://github.com/ideal-world/spacegate.git
cd spacegate
```

### 1.3 常用构建命令

```bash
# debug 构建（开发时使用）
cargo build

# release 构建：本地 Linux 二进制（启用动态库插件支持）
cargo make build-spacegate-linux

# release 构建：Kubernetes 用途（内置所有插件）
cargo make build-spacegate-k8s

# 构建 Docker 镜像（需先设置 DOCKER_REPO 和 DOCKER_VERSION）
DOCKER_REPO=myrepo DOCKER_VERSION=v1.0 cargo make build-k8s-docker

# 构建并安装到本地 Linux 系统
cargo make install-spacegate

# 构建管理服务器
cargo make build-spacegate-admin

# 安装管理服务器
cargo make install-spacegate-admin
```

### 1.4 代码规范检查

提交代码前必须通过以下检查：

```bash
# 格式化检查
cargo fmt --all --check

# Lint 检查（须开启所有 feature）
cargo clippy --all-features
```

格式化配置见 [`rustfmt.toml`](../../rustfmt.toml)（`max_width = 180`），Lint 配置见 [`clippy.toml`](../../clippy.toml)。

### 1.5 运行测试

```bash
# 运行所有单元测试
cargo test --all

# 运行内核集成测试（需要本地网络支持）
cargo test -p spacegate-kernel

# 运行特定集成测试
cargo test -p spacegate-kernel --test test_websocket
cargo test -p spacegate-kernel --test test_h2 --features axum/http2
```

内核集成测试覆盖：HTTP/2、HTTPS、WebSocket、Multipart、同端口多协议。

---

## 2. 项目架构与 Crate 职责

### 2.1 Workspace 成员

Spacegate 是一个 Cargo workspace，成员如下：

```
spacegate/
├── crates/
│   ├── kernel/          # 核心网关引擎
│   ├── model/           # 配置数据模型
│   ├── plugin/          # 插件系统
│   ├── config/          # 配置存储抽象
│   ├── shell/           # 启动与运行时管理
│   └── extension/
│       ├── axum/        # Axum HTTP 扩展（可选）
│       └── redis/       # Redis 连接池扩展（可选）
├── binary/
│   ├── spacegate/       # 网关可执行入口
│   └── admin-server/    # 配置管理 REST API
└── examples/
    ├── sayhello/        # 动态库插件示例
    ├── socks5-proxy/    # Socks5 代理示例
    └── mitm-proxy/      # 中间人代理示例
```

### 2.2 各 Crate 职责

#### `spacegate-kernel`

核心引擎，不依赖插件和配置系统。职责：

- HTTP/HTTPS 协议栈（基于 hyper + rustls）
- 请求路由（主机名匹配树、路径/方法/Header 匹配）
- 负载均衡（Random 权重随机、IpHash 基于客户端 IP）
- 后端代理（HTTP/1.1、HTTP/2、WebSocket、静态文件）
- Layer/Middleware 抽象（基于 tower-layer）
- 热重载机制（`ArcSwap` + `Reloader`）
- 请求扩展（`Reflect`、`PeerAddr`、`EnterTime`、`GatewayName` 等）

#### `spacegate-model`

配置数据契约层，定义所有配置结构体，不包含任何运行时逻辑。详见 [§5 配置系统](#5-配置系统)。

#### `spacegate-plugin`

插件系统。职责：

- 定义 `Plugin` trait（插件接口）
- 维护 `PluginRepository`（全局插件仓库，存储插件定义与实例）
- 定义四层挂载点 `MountPointIndex`（Gateway / HttpRoute / HttpRouteRule / HttpBackend）
- 内置插件（限流、重写、重定向、Header 修改、维护模式等）
- 动态库插件加载（`dylib` feature）

#### `spacegate-config`

配置存储抽象层。通过 trait 屏蔽底层存储差异，支持运行时配置变更监听。详见 [§5.2 配置后端](#52-配置后端)。

#### `spacegate-shell`

启动与运行时管理。提供四种 `startup_*` 函数，负责：

- 拉取初始配置并构建网关实例（`RunningSgGateway`）
- 监听配置变更事件，动态更新路由/插件（热重载）
- 管理网关生命周期（优雅关闭）
- 可选：启动 Axum 管理 HTTP 服务器

#### `spacegate-ext-axum` / `spacegate-ext-redis`

可选扩展：

- **ext-axum**：提供全局 Axum HTTP 服务器单例（`GlobalAxumServer`），插件可向其注册自定义 HTTP 路由，用于暴露管理/监控接口。
- **ext-redis**：提供 Redis 连接池（`RedisClient`）和全局客户端仓库（`RedisClientRepo`），支持基于 Redis 的分布式插件（限流、动态路由等）。

#### `binary/spacegate`

网关可执行程序入口，解析命令行参数后调用 `spacegate-shell` 的 `startup_*` 函数。

```bash
# 文件配置模式
./spacegate --config file:/etc/spacegate

# Kubernetes 模式
./spacegate --config k8s:default

# Redis 模式
./spacegate --config redis://127.0.0.1:6379

# 静态配置模式（JSON 文件）
./spacegate --config static:/etc/spacegate/config.json

# 加载动态库插件
./spacegate --config file:/etc/spacegate --plugins /lib/spacegate/plugins
```

#### `binary/admin-server`

配置管理 REST API 服务器，提供 CRUD 接口操作网关配置，支持文件系统和 Kubernetes 两种后端。

### 2.3 Crate 依赖关系

```
spacegate-model  ←──────────────────────────────────┐
      ↑                                               │
spacegate-kernel  ←───────────────────────────────   │
      ↑                    ↑                          │
spacegate-ext-redis    spacegate-ext-axum             │
      ↑                    ↑                          │
spacegate-plugin  ─────────────────────────────────→─┘
      ↑
spacegate-config
      ↑
spacegate-shell
      ↑
  binary/*
```

---

## 3. 请求生命周期

### 3.1 完整数据流

```
客户端
  │
  │ TCP 连接
  ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel::listener                                             │
│  协议嗅探：HTTP ("\x47\x45\x54..." 等方法前缀)              │
│           HTTPS ("\x16\x03" TLS record header)              │
│           HTTPS → rustls 解密                               │
└──────────────────────────┬──────────────────────────────────┘
                           │ hyper HTTP/1.1 or HTTP/2
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel::service::HyperServiceAdapter                         │
│  • 注入 PeerAddr（客户端地址）                              │
│  • 创建 Reflect 扩展（请求处理链追踪）                      │
│  • 注入 EnterTime（进入时间）                               │
│  • Body 转换为 SgBody                                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ [插件层] Gateway 级插件（洋葱模型外层）                     │
│  典型用途：认证、请求追踪、全局限流                         │
│  注入 GatewayName 扩展                                      │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel::service::http_gateway::GatewayRouter                 │
│  • HostnameTree 匹配虚拟主机                                │
│  • 按优先级遍历 HttpRoute 列表                              │
│  • 执行 SgHttpRouteMatch（路径/方法/Header 匹配）           │
│  • 匹配成功后执行 URL Rewrite                               │
└──────────────────────────┬──────────────────────────────────┘
                           │ (route_index, rule_index)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ [插件层] HttpRoute 级插件                                   │
│  典型用途：路由级鉴权、流量镜像                             │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ [插件层] HttpRouteRule 级插件                               │
│  典型用途：基于路径的特定逻辑（限流、Header 修改）          │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel::Balancer（负载均衡）                                 │
│  • Random：按 weight 权重随机选择后端                       │
│  • IpHash：按客户端 IP 哈希选择后端                         │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ [插件层] HttpBackend 级插件                                 │
│  典型用途：后端级协议转换、重试、熔断                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ kernel::backend_service::HttpBackendService                  │
│  • 修改请求 URI（scheme/host/port/path）                    │
│  • 注入 X-Forwarded-For                                    │
│  • 分支处理：                                               │
│    - BackendHost::Host → HTTP(S) 代理转发                   │
│    - BackendHost::K8sService → Kubernetes Service 转发      │
│    - BackendHost::File → 静态文件服务                       │
│  • WebSocket Upgrade 透明转发                               │
└──────────────────────────┬──────────────────────────────────┘
                           │ 后端响应
                           ▼
              插件链逆序处理响应（洋葱模型）
              Backend → RouteRule → Route → Gateway
                           │
                           ▼
                         客户端
```

### 3.2 四层插件挂载点

| 挂载层级 | `MountPointIndex` 变体 | 作用域 | 典型用途 |
|----------|----------------------|--------|---------|
| **Gateway** | `Gateway { gateway }` | 所有请求 | 全局认证、请求追踪、访问日志 |
| **HttpRoute** | `HttpRoute { gateway, route }` | 匹配该路由的请求 | 虚拟主机级鉴权、流量镜像 |
| **HttpRouteRule** | `HttpRouteRule { gateway, route, rule }` | 匹配该规则的请求 | 路径级限流、Header 修改 |
| **HttpBackend** | `HttpBackend { gateway, route, rule, backend }` | 转发到特定后端时 | 重试、熔断、协议转换 |

插件在每一层均以**洋葱模型**执行：请求时从外层到内层，响应时从内层到外层。

---

## 4. 插件开发教程

### 4.1 Plugin Trait

```rust
pub trait Plugin: Any + Sized + Send + Sync {
    /// 插件唯一标识符（建议使用 kebab-case）
    const CODE: &'static str;

    /// 是否单实例模式（true 表示全局只有一个实例）
    const MONO: bool = false;

    /// 插件元数据（描述、版本等）
    fn meta() -> PluginMetaData { PluginMetaData::default() }

    /// 请求处理函数（核心业务逻辑）
    fn call(&self, req: SgRequest, inner: Inner)
        -> impl Future<Output = Result<SgResponse, BoxError>> + Send;

    /// 工厂方法：从 PluginConfig 创建插件实例
    fn create(plugin_config: PluginConfig) -> Result<Self, BoxError>;

    /// 注册到 PluginRepository（通常无需覆盖）
    fn register(repo: &PluginRepository) { ... }
}
```

`PluginConfig` 结构：

```rust
pub struct PluginConfig {
    pub id: PluginInstanceId {
        pub code: String,   // 与 Plugin::CODE 一致
        pub name: String,   // 实例名称（同一插件可有多个实例）
    },
    pub spec: JsonValue,    // 插件配置（JSON），由开发者自定义
}
```

### 4.2 最简插件示例

以下示例在每个响应中添加 `Server` 头：

```rust
use spacegate_plugin::{Plugin, SgRequest, SgResponse, Inner, BoxError, PluginConfig};

pub struct ServerHeaderPlugin {
    header_value: String,
}

impl Plugin for ServerHeaderPlugin {
    const CODE: &'static str = "server-header";

    async fn call(&self, req: SgRequest, inner: Inner) -> Result<SgResponse, BoxError> {
        let mut resp = inner.call(req).await;
        resp.headers_mut().insert("server", self.header_value.parse()?);
        Ok(resp)
    }

    fn create(config: PluginConfig) -> Result<Self, BoxError> {
        let header_value = config.spec
            .get("header_value")
            .and_then(|v| v.as_str())
            .unwrap_or("spacegate")
            .to_string();
        Ok(Self { header_value })
    }
}
```

对应的插件配置 JSON：

```json
{
  "code": "server-header",
  "name": "my-server-header",
  "spec": {
    "header_value": "my-gateway/1.0"
  }
}
```

### 4.3 带结构化配置的插件

推荐使用 `serde` 反序列化插件配置，并在开启 `schema` feature 时自动生成 JSON Schema：

```rust
use serde::{Deserialize, Serialize};
use spacegate_plugin::{Plugin, PluginConfig, SgRequest, SgResponse, Inner, BoxError};

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MyPluginConfig {
    pub timeout_ms: u64,
    pub allowed_methods: Vec<String>,
}

pub struct MyPlugin {
    config: MyPluginConfig,
}

impl Plugin for MyPlugin {
    const CODE: &'static str = "my-plugin";

    fn create(plugin_config: PluginConfig) -> Result<Self, BoxError> {
        let config = serde_json::from_value::<MyPluginConfig>(plugin_config.spec)?;
        Ok(Self { config })
    }

    async fn call(&self, req: SgRequest, inner: Inner) -> Result<SgResponse, BoxError> {
        // 插件逻辑
        inner.call(req).await.map(Ok)?
    }

    #[cfg(feature = "schema")]
    fn schema_opt() -> Option<schemars::schema::RootSchema> {
        Some(schemars::schema_for!(MyPluginConfig))
    }
}
```

### 4.4 使用 `PluginError` 返回错误响应

当插件需要返回非 200 的错误响应时，使用 `PluginError` 而不是直接返回 `Err`：

```rust
use spacegate_plugin::{PluginError, Plugin};
use hyper::StatusCode;

// 返回 403 响应（不会触发 500 包装）
return Ok(PluginError::status_code::<MyPlugin>(
    StatusCode::FORBIDDEN,
    "access denied",
).into_response());

// 返回内部错误（会触发 500 包装）
return Err(PluginError::internal_error::<MyPlugin>("something went wrong").into());
```

### 4.5 内置插件列表

| 插件 CODE | 功能 | 依赖 feature |
|-----------|------|-------------|
| `header-modifier` | 添加/删除请求或响应 Header | `header-modifier` |
| `rewrite` | 重写请求 URL 路径和 Host | `rewrite` |
| `redirect` | HTTP 重定向 | `redirect` |
| `inject` | 向请求注入固定数据 | `inject` |
| `retry` | 请求重试 | `retry` |
| `limit` | 速率限制（基于 Redis） | `limit`（含 `cache`） |
| `maintenance` | 维护模式（返回固定响应） | `maintenance` |
| `set-version` | 强制设置 HTTP 协议版本 | `set-version` |
| `set-scheme` | 修改请求 URI scheme | `set-scheme` |
| `status` | 返回网关状态信息 | `status` |
| `east-west-traffic-white-list` | 东西向流量 IP 白名单 | `east-west-traffic-white-list` |
| `static-resource` | 静态文件服务 | — |

启用所有内置插件：

```toml
[dependencies]
spacegate-plugin = { version = "...", features = ["full"] }
```

### 4.6 注册插件

#### 方式一：在程序启动时注册

```rust
use spacegate_plugin::PluginRepository;

fn main() {
    let repo = PluginRepository::global();
    ServerHeaderPlugin::register(repo);
    MyPlugin::register(repo);

    // 启动网关
    spacegate_shell::startup_file("./config").await?;
}
```

#### 方式二：动态库插件（`.so` / `.dylib` / `.dll`）

动态库插件通过 `#[no_mangle]` 导出注册函数：

```toml
# Cargo.toml
[lib]
crate-type = ["dylib"]

[dependencies]
spacegate-plugin = { version = "...", features = ["dylib"] }
```

```rust
// src/lib.rs
use spacegate_plugin::{Plugin, PluginRepository};

pub struct SayHelloPlugin;

impl Plugin for SayHelloPlugin {
    const CODE: &'static str = "say-hello";
    // ... 实现 call 和 create
}

// 必须导出此函数，名称固定
#[no_mangle]
pub fn register(repo: &PluginRepository) {
    SayHelloPlugin::register(repo);
}
```

网关二进制在启动时通过 `--plugins <dir>` 参数加载目录下所有动态库：

```bash
./spacegate --config file:/etc/spacegate --plugins /lib/spacegate/plugins
```

完整示例见 [`examples/sayhello/`](../../examples/sayhello/)。

---

## 5. 配置系统

### 5.1 配置数据模型

配置模型形成如下层次结构（定义在 `spacegate-model`）：

```
Config
└── gateways: BTreeMap<gateway_name, ConfigItem>
    └── ConfigItem
        ├── gateway: SgGateway
        │   ├── name: String                    # 网关唯一名称
        │   ├── parameters: SgParameters
        │   │   ├── redis_url: Option<String>   # Redis 连接 URL
        │   │   ├── log_level: Option<String>   # 日志级别
        │   │   ├── ignore_tls_verification: Option<bool>
        │   │   └── enable_x_request_id: Option<bool>
        │   ├── listeners: Vec<SgListener>
        │   │   ├── name: String
        │   │   ├── ip: Option<IpAddr>           # 默认 0.0.0.0
        │   │   ├── port: u16
        │   │   └── protocol: SgProtocolConfig
        │   │       ├── Http
        │   │       └── Https { tls: SgTlsConfig { mode, key, cert, http2 } }
        │   └── plugins: Vec<PluginInstanceId>  # 网关级插件引用
        └── routes: BTreeMap<route_name, SgHttpRoute>
            └── SgHttpRoute
                ├── route_name: String
                ├── hostnames: Option<Vec<String>>  # 虚拟主机匹配
                ├── priority: i16                   # 值越大优先级越高，默认 1
                ├── plugins: Vec<PluginInstanceId>  # 路由级插件引用
                └── rules: Vec<SgHttpRouteRule>
                    └── SgHttpRouteRule
                        ├── matches: Option<Vec<SgHttpRouteMatch>>
                        │   # 路径匹配、方法匹配、Header 匹配
                        ├── plugins: Vec<PluginInstanceId>  # 规则级插件引用
                        ├── timeout_ms: Option<u32>
                        └── backends: Vec<SgBackendRef>
                            └── SgBackendRef
                                ├── host: BackendHost
                                │   ├── Host(String)           # IP 或域名
                                │   ├── K8sService(K8sServiceData)
                                │   └── File { path: String }  # 静态文件目录
                                ├── port: Option<u16>
                                ├── protocol: Option<SgBackendProtocol>  # http/https
                                ├── weight: Option<u16>        # 负载均衡权重
                                ├── timeout_ms: Option<u32>
                                └── plugins: Vec<PluginInstanceId>  # 后端级插件引用
```

`PluginInstanceId` 引用全局插件实例池：

```
Config
└── plugins: PluginInstanceMap
    └── Map<PluginInstanceId, JsonValue>  # 插件 spec 配置
```

### 5.2 配置后端

`spacegate-config` 通过以下 trait 抽象配置存储：

| Trait | 职责 |
|-------|------|
| `Retrieve` | 读取网关、路由、插件配置 |
| `Create` | 创建新的网关/路由/插件配置 |
| `Update` | 更新已有配置 |
| `Delete` | 删除配置 |
| `CreateListener` | 获取初始配置并返回配置变更事件流 |

支持的后端实现：

| 后端 | Cargo Feature | Shell 启动函数 | 适用场景 |
|------|---------------|----------------|---------|
| **文件系统** (`Fs`) | `fs` | `startup_file(conf_dir)` | 本地开发、单机部署 |
| **Kubernetes** (`K8s`) | `k8s` | `startup_k8s(namespace)` | 云原生 K8s 部署 |
| **Redis** (`Redis`) | `cache` / `redis` | `startup_redis(url)` | 多实例分布式部署 |
| **内存** (`Memory`) | — | `startup_static(config)` | 库化嵌入、测试 |

### 5.3 配置变更事件

配置后端通过 `CreateListener` 返回事件流，驱动热重载：

```rust
pub enum ConfigEventType { Create, Update, Delete }

pub enum ConfigType {
    Gateway { name: String },
    HttpRoute { gateway: String, route: String },
    Plugin { id: PluginInstanceId },
}

pub struct ListenEvent {
    pub event_type: ConfigEventType,
    pub config_type: ConfigType,
}
```

Shell 层监听这些事件后，会对相应的 `RunningSgGateway` 进行增量更新，无需重启网关进程。

### 5.4 文件系统配置格式示例

以 JSON 格式（`config.json`）为例：

```json
{
  "gateways": {
    "my-gateway": {
      "gateway": {
        "name": "my-gateway",
        "listeners": [
          {
            "name": "http",
            "port": 8080,
            "protocol": "Http"
          }
        ],
        "plugins": []
      },
      "routes": {
        "api-route": {
          "route_name": "api-route",
          "hostnames": ["api.example.com"],
          "priority": 10,
          "plugins": [],
          "rules": [
            {
              "matches": [
                { "path": { "type": "PathPrefix", "value": "/api" } }
              ],
              "backends": [
                {
                  "host": { "Host": "backend-service" },
                  "port": 3000,
                  "weight": 1
                }
              ]
            }
          ]
        }
      }
    }
  },
  "plugins": {}
}
```

---

## 代码提交规范

- 提交前确保通过 `cargo fmt --all --check` 和 `cargo clippy --all-features`
- 新增功能请同步添加测试
- 内置插件代码位于 `crates/plugin/src/plugins/`，实现 `Plugin` trait 并在 `crates/plugin/src/plugins.rs` 中注册
- 禁止在非测试代码中使用 `unwrap()`（Clippy 规则强制执行）
- 禁止使用 `dbg!`、`todo!`、`unimplemented!`（Clippy 规则强制执行）

## 参考资料

- [Kubernetes Gateway API 规范](https://gateway-api.sigs.k8s.io/)
- [hyper 文档](https://docs.rs/hyper)
- [tower-layer 文档](https://docs.rs/tower-layer)
- [Cargo Make 文档](https://sagiegurari.github.io/cargo-make/)
- [`docs/k8s/installation.md`](../k8s/installation.md) — Kubernetes 部署指南
