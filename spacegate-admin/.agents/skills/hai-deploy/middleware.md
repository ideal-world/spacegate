# 部署环境与中间件

## 环境先行

`hai deploy` 首先输入部署环境，或用 `hai deploy --env test` 指定。`.env.test` / `.env.prod` 必须由用户先创建；缺失时停止，不代替用户创建一个空文件继续部署。指定要部署的目录，例如 `hai deploy packages/my-app-serv --env test`，该目录必须对应 lockfile 的一个 importer；不遍历 apps/packages 或猜测 fullstack 布局。环境文件默认取当前目录（有 `.env` 时）或 lockfile 根目录，必要时传 `--env-dir`。

环境文件遵循项目的六文件规范：`.env.example` 为带说明/枚举/范围的完整清单，`.env` 为公共基础，`.env.dev/test/prod` 为公共环境覆盖，以上五类提交 Git；`.env.local` 为忽略 Git 的本机私有覆盖。加载顺序为进程 > `.env.local` > 目标环境 > `.env`。真实密码、API Key、含认证信息的连接 URL 写 `.env.local`，公共后端和地址写目标环境文件；CLI 仅报告保存结果，不打印环境变量名或值。调用 deploy API 时显式传 `{ environment: 'test', envDir: '/workspace/app' }`，不要用调用者的 `NODE_ENV` 猜测部署目标。

## 识别与逐项选择

数据库连接保存前复用所选后端的公开配置 Schema；MySQL `ssl` 仅 `false` 或严格对象，`{}` 默认验证证书链和 DNS 身份，`ca/cert/key` 为 PEM 内容且 `cert/key` 必须成对；私钥仅进入私有环境。PostgreSQL 保留其布尔值/字符串模式/对象契约。CLI 内联 `@h-ai/reldb/schemas` 与 Zod，无需为配置校验给目标应用额外安装数据库包或驱动；部署本身仍需已安装的 `@h-ai/deploy`，实际运行应用仍需其数据库依赖。

先用 `deploy.scan(appDir, { environment, envDir })` 核对实际依赖、配置与环境值。扫描从目标 workspace 的 `pnpm-lock.yaml` importer 出发，沿 dependencies/optionalDependencies 遍历，排除 devDependencies、peerDependencies 和其他应用未被引用的依赖；workspace link 只沿运行依赖继续。pnpm 12 的工具链 YAML 文档不能混入应用图。lockfile、目标 importer 或依赖节点缺失时先执行 `pnpm install` 修复；依赖图不读取 package.json、pnpm-workspace.yaml 或项目目录布局。配置文件决定已识别模块的后端，保留的旧配置不能自行启用未依赖的模块。

`ScanResult.haiModules` 列出所有实际运行依赖中的 Hai 模块；`ScanResult.middlewares` 提供服务、后端、来源、配置路径和变量前缀；连接配置可能含凭据，不能整体记入日志。`DEPLOY_MIDDLEWARE` 是后端和连接字段的公共目录，字段名称、类型与约束须与模块 Schema 同步。

| 来源                                       | 后端选项                              | 默认 Compose 行为                                             | 云自动开通                          |
| ------------------------------------------ | ------------------------------------- | ------------------------------------------------------------- | ----------------------------------- |
| reldb；IAM/audit；启用 DB 日志的 scheduler | SQLite / MySQL / PostgreSQL           | SQLite 数据卷；mysql / postgres 服务                          | PostgreSQL → Neon                   |
| cache；IAM；scheduler 可选分布式锁         | memory / Redis                        | memory 无服务；redis 服务                                     | Redis → Upstash                     |
| storage                                    | local / S3                            | 本地数据卷；RustFS（默认）/MinIO + bucket 初始化              | S3 → Cloudflare R2                  |
| vecdb                                      | LanceDB / pgvector / Qdrant / Chroma  | 本地卷或对应容器                                              | 自定义已有服务                      |
| IAM 可选目录认证                           | disabled / LDAP                       | 禁用无服务；OpenLDAP                                          | 自定义已有目录                      |
| Reach Provider 数组                        | console / SMTP / HTTP API；阿里云 SMS | console 无服务；SMTP 用 Mailpit 捕获测试邮件；HTTP/SMS 自定义 | SMTP → Resend；SMS → 阿里云凭据接入 |

`serv` 声明 AI peer 不代表应用启用了 AI，不应因此安装 AI/ONNX/CUDA 或开通向量服务。LDAP 需要在应用安装 IAM 的可选 peer `ldapts`，并显式设置 `login.ldap=true`；安装 IAM 不代表启用了 LDAP。Reach 每个 Provider 保留独立数组索引及名称；通用 Console/HTTP API 按应用模板约定将 `name: sms` 归为短信，其他名称归为邮件服务，SMTP 和 aliyun-sms 按实际 type 分类，不能把 HTTP 回调当作 SMTP。email/sms 的 Console 均只输出日志，不实际投递。默认目录没有业务账号；Mailpit 不会对外投递生产邮件。生产应配置企业 LDAP/SMTP，或使用对应云服务。

每项都展示当前生效后端和用途，例如 PostgreSQL，再选择：

1. **使用默认配置**：选择后端；Compose 生成匹配容器/卷，云平台调用已支持的 Provisioner，并保存实际连接配置。MySQL、LDAP、向量库等没有云自动开通器时，明确提示改用自定义连接；不能声称已经创建云资源。Resend 需要已验证的发件域名，R2 需要已有 S3 访问密钥，阿里云短信需要签名/模板与额度。
2. **使用自定义配置**：逐项输入所选后端的连接字段，展示字段用途与约束，校验必填项、端口、布尔和 URL；凭据用密码输入。保存配置后跳过该服务的开通与 sidecar。
3. **保持不变**：仅在目标 `.env.<mode>` 显式声明当前后端及完整连接配置时出现；私有凭据可来自 `.env.local`/进程。选择后不修改该项、不认证其 Provisioner、不创建或移除已有中间件容器，继续下一项。只有 `.env` 或其他环境有配置不满足此条件。

逐项配置前提供“环境配置已完备，直接部署”。选择后跳过全部中间件提示、变量改写、云资源与 sidecar 创建，保留已有服务；仍校验已有构建产物并执行部署及健康检查。命令行使用唯一的 `--skip-middleware`，SDK 使用 `skipMiddleware`。不要因为缺少某项中间件的目标文件声明而阻止用户主动跳过。

`--yes` 用于已授权的自动化：目标环境已有完整配置时保持不变，否则使用当前后端默认配置；不支持的云后端或缺少必需凭据直接失败。自动化直接部署用 `hai deploy packages/my-app-serv --env prod --yes --skip-middleware`。跳过不作为配置完整性或连接成功的证明。

直接部署的目标不完整时，先引导选择 SSH/Vercel。SSH 逐项输入主机、端口、用户名、私钥文件路径、known_hosts 路径、远程目录/运行时及容器/主机端口；端口范围 1–65535。密钥路径留空使用 ssh-agent/默认密钥，known_hosts 留空使用 OpenSSH 默认文件。完整目标自动复用；修改 `.env.local` 的 `HAI_DEPLOY_PROVIDER` 可切换连接。首次填写的整个目标对象（包括地址、端口、路径、Token）只保存 `.env.local` 并添加 Git 忽略，不把机器信息写公共环境文件。`--bundle-only` 不询问 SSH，连接信息可在后续上传前填写到生成的 `deploy-target.json`。SDK 使用 `saveDeploymentEnvironment(snapshot, variables, { private: true })` 可显式保存本机参数。禁止读取、复制或打印私钥内容。

### Hai 模块与中间件的关系

识别依据是运行图中的所有节点，不按应用直接依赖硬编码消费者名单。下面按当前公开依赖和功能说明；新增模块/后端时同步此表、连接目录与真实运行测试。

| 模块                                                                        | 中间件来源或外部能力                                                                                                                                                                |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| reldb / cache / storage / vecdb                                             | 分别拥有关系库、缓存、对象存储、向量库连接契约                                                                                                                                      |
| iam / kit                                                                   | IAM 运行依赖 audit/reldb/cache/crypto；kit 经 IAM 继承。LDAP 按 login.ldap 开关启用，Reach peer 只有另行实际依赖时进入运行图                                                        |
| audit / payment                                                             | audit 经 reldb 持久化；Stripe payment 直接依赖先初始化的共享 reldb 保存渠道映射和幂等结果，审计可选。微信、支付宝、Stripe 是已有商户 API，沿 payment 配置接入，不生成假云服务或容器 |
| scheduler / reach                                                           | 两者实际依赖 reldb/cache；Reach 另按启用的 SMTP/HTTP/短信渠道配置                                                                                                                   |
| ai                                                                          | 实际依赖 reldb/vecdb/datapipe。LLM、图像、音频端点及模型来自 AI 配置，不能仅凭包名选择模型镜像或 CUDA；不自动创建付费厂商账号                                                       |
| crypto / serv                                                               | crypto 的可选运行依赖按 lockfile 解析，serv 经 crypto 继承；serv 的 IAM/storage/AI peer 不作为启用证据                                                                              |
| core / datapipe / api-contract / api-client / ui / capacitor / cli / deploy | 本身没有额外专用中间件；仍沿实际运行依赖继续遍历，例如 api-client/capacitor 的 crypto 分支。UI/设备能力、部署目标/Provisioner 凭据按各自配置处理                                    |

S3 是协议，不是某个容器产品。默认配置时另选 RustFS/MinIO，保存到 HAI_DEPLOY_CONTAINER_S3ENGINE（rustfs | minio，默认 rustfs），应用始终写 HAI_STORAGE_TYPE=s3 或 HAI_STORAGE。自定义 AWS S3、OSS、R2 等端点不生成 sidecar。镜像使用与 storage 集成测试一致的 RustFS 官方版本和 MinIO 社区版本；存储桶初始化复用应用镜像里的 S3 SDK，无需额外 mc 镜像，必须幂等且不能吞掉失败。

AI 使用 MySQL 的首次建表需 CREATE 与 INDEX 权限，节点可并发初始化；仅明确重复索引且重查真实结构一致时继续，错误同名索引、权限和其他 DDL 故障必须阻止应用启动。DDL 不回滚，失败后检查初始化结果、逆序清理连接，再修复结构/权限重试，不能删库或吞错视为成功。

## 连接字段

以下是自定义向导的连接字段；完整可用项、枚举与范围仍以项目 `.env.example` 和公开 Schema 为准。变量前缀为 `HAI_DB`、`HAI_CACHE`、`HAI_STORAGE`、`HAI_VECDB`、`HAI_IAM_LDAP`；Reach 使用实际索引 `HAI_REACH_PROVIDERS_<index>`。camelCase 不拆词，例如 `bindPassword` → `BINDPASSWORD`。

| 后端               | 连接字段                                                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------ |
| SQLite             | database（文件路径或 `:memory:`）                                                                                                    |
| MySQL / PostgreSQL | url（优先）、host、port（1–65535）、database、user、password、ssl                                                                    |
| Redis              | url（优先）、host、port、password、db、tls、keyPrefix、connectTimeout、commandTimeout                                                |
| local / LanceDB    | root / path                                                                                                                          |
| S3                 | endpoint、bucket、region、accessKeyId、secretAccessKey、forcePathStyle                                                               |
| Qdrant / Chroma    | url；Qdrant 可选 apiKey；Chroma apiKey 用于 Bearer 自托管网关，URL 仅支持 HTTP(S) 根端点，不提供 Cloud 契约                          |
| pgvector           | url（优先）或 host/port/user/password，以及 database                                                                                 |
| LDAP               | url、bindDn、bindPassword、searchBase、searchFilter、usernameAttribute、emailAttribute、displayNameAttribute、useTls、connectTimeout |
| SMTP               | host、port、secure、user、pass、from                                                                                                 |
| HTTP API           | url、method（POST / PUT）、headers（JSON）、timeout                                                                                  |
| 阿里云 SMS         | accessKeyId、accessKeySecret、signName、endpoint                                                                                     |

## 离线准备与验收

构建与部署可分开，也可通过 pnpm run deploy --build 明确串联。构建保持环境无关；部署选择环境并只消费所选镜像。完整包自动包含 Compose 自建中间件镜像，因此导出阶段需要本地 Podman/Docker，缺镜像在打包阶段准备，不能留给隔离服务器下载。

部署缓存、带版本 tar 与 -deploy/ 目录默认位于工程根 .hai/deploy/<环境>/；--output 可显式指定其他 tar 路径，相对命令执行目录解析。--bundle-only 只生成交付物，不连接服务器。操作目录含 Windows deploy.ps1、Linux/macOS deploy.sh 和带逐项配置说明的 deploy-target.json；运行上传脚本前先修改并核对该 JSON，两个脚本均读取此文件。本机运行 deploy.sh 需 sh、awk、ssh、scp；sh deploy.sh 主机 [端口] [用户] 可覆盖连接地址，HAI_DEPLOY_IDENTITY_FILE 和 HAI_DEPLOY_KNOWN_HOSTS_FILE 可覆盖两脚本的私钥及指纹文件路径。隔离服务器搬运并解包后执行 sh apply.sh。每步失败返回非零，结果写 result-<部署ID>.json；只有 HTTP 检查通过才更新服务器 current。

部分更新通过 --services 指定，使用服务器成功基线保留未选服务和中间件凭据。首次安装不能使用部分包。storage-init 复用应用镜像及 S3 SDK 检查/创建桶，不清空数据；只有涉及存储初始化时执行，并检查退出码。仅更新 storage 仍需含 app 镜像的制品；最近为 web-only 时，用 --artifact-dir 选择原完整制品，或先 pnpm build --target docker --images app。app 镜像仅用于初始化，不会更新 app 服务。失败恢复尝试还原容器和配置，保留数据卷，不回退已写业务数据。

服务器需要 Linux、Docker/Podman、Compose、sh、tar、sha256sum、awk、curl、util-linux flock、支持 flock 的持久化文件系统和容器/目录权限。部署和恢复使用固定 `.deploy.lock` 内核锁，不可删除/替换或按时间抢锁；活跃子命令继承 FD，全部持有进程退出或重启后自动释放。强杀/断电后的执行状态需核对结果文件、current 和容器，不能将锁释放当作恢复成功。完整交付包含自建服务镜像；自定义外部连接仍需准备服务、账号和网络可达性。云 Provider/Provisioner 需要外网，不能用于完全离线。

生成的 `web.conf` 在 API location 设 `client_max_body_size 2129920`（2 MiB + 32 KiB），用 large_client_header_buffers 4 32k / proxy_buffer_size 32k 容纳 X-Hai-Transport 认证头，并配套 proxy_buffers 8 32k，保证关闭 buffering 时也能通过 Nginx 缓冲区配置校验。请求/响应 proxy buffering 保持关闭；这与 transport 密文预算一致，避免 Nginx 默认上限拦截合法的 1 MiB 明文编码结果。应用仍执行握手 16 KiB、公钥 4 KiB、明文 1 MiB 的有界读取。自有反向代理须允许相同密文预算并传播取消，不替代应用字节限制；大文件上传使用独立路径和明确预算。

完全离线构建另需基础镜像、Linux 依赖缓存、pnpm 和匹配驱动/Node ABI/架构/libc 的原生预编译包。构建可用 --sqlite-prebuild-mirror，Podman 代理不可达可用 --no-http-proxy；不要改系统代理或让 deploy 下载编译环境。生产 HTTPS、CORS、证书、DNS 另行准备。实际断网验收登录、刷新、重启和数据持久化。

## 命令输出

内部 `runCommand` / `SshExecutor.exec` 先 UTF-8 解码，再对每条逻辑流跨块精确脱敏；成功 stdout/stderr、错误、onOutput 均为安全结果。`secrets` 不推断全部进程环境，调用方列出秘密及可能输出的编码/转换值；总长不超过 1 Mi UTF-16 单元，脱敏异常失败关闭且不暴露原 cause。`maxCaptureChars` 默认 1048576、最大 16777216，捕获尾部并返回 stdoutTruncated/stderrTruncated，结构化解析先拒绝截断。onOutput 支持 Promise，顺序等待并保持 pipe 背压；异常回调使执行失败。异常终止不释放未完成的秘密尾部。部署进度仅解析每行前 512 字符。超时不能证明远端工作已终止。

deadline 包括 helper 启动和输出处理。异常后 `terminationTimeoutMs` 另等待本地终止核验，默认 10000、上限 60000 毫秒。POSIX 用独立继承进程组；Windows 用隐藏且不加载 profile 的 PowerShell/.NET Job，挂起创建 → 分配成功 → 恢复，禁止 breakaway，helper/编译/分配失败拒绝执行。已脱离组的系统 daemon、现有容器和远端服务不在本地确认范围。只有受管理 Job/组无活动成员才是 confirmed；未证明为 unknown，启动前为 not-started。输入、回调和流异常同样经过此路径，已经进入的回调不能撤销，但不再交付新块。

公开 `getDeployCommandState(result.error)` 在失败分支读取原因、本地状态、远端状态、deploymentId 和 resultFile。SSH 中断默认为远端 unknown；docker-ssh 新连接只读解析原子 `result-<ID>.json`，完整/未截断/ID 匹配才为 finished，并返回实际阶段、退出码。finished 指脚本及恢复结束，容器可以继续运行；即使终态为 0，原连接/输出失败仍保留。缺失、旧记录、损坏或重连失败保持 unknown，不自动重试写入。按 ID 核对步骤、current 和服务，再恢复。
