---
name: hai-build
description: "构建或排查 hai 应用的源码、容器镜像和桌面产物；识别工程架构、依赖与对应构建入口。"
---

# hai-build

## 能力契约

| 项目 | 契约 |
| --- | --- |
| 能力 | 定位 hai 应用架构、模块依赖、初始化顺序、构建模式和对应技能 |
| 适用场景 | 识别应用架构、跨模块依赖或构建入口 |
| 输入 | README、package.json、应用类型、现有 config 与目标 |
| 输出 | 适用技能、最小修改路径和按范围选择的验证命令 |
| 限制 | 先识别 API workspace、SvelteKit、SPA 或原生 App；不强行转换架构，不给纯函数模块添加生命周期。 |

## 使用路径

- 先读当前 package.json、lockfile 与 README，识别 SvelteKit / API workspace / fullstack / Vite SPA / 原生 App，按实际 scripts 验证。
- 升级 hai-framework、比较版本差异或同步框架 skills 时，使用 [hai-framework-sync](../hai-framework-sync/SKILL.md) 完成版本发现、差异分析、选择/阻塞处理、迁移与验证。
- 功能开发先用 [development.md](../hai-app-create/development.md) 把需求匹配到当前版本的框架能力与配置；再读取所选模块 skill 和公开类型。
- 有生命周期的模块按真实依赖初始化、检查结果并反序清理；core、纯函数和工厂不套用相同生命周期。
- 构建任务检查当前 engines、适配器、缓存与目标产物，不能照搬框架仓库私有构建命令。
- 多项门禁按 [分轮验证与集中修复](../hai-app-tests/validation.md) 先收集结果再集中修复；遵守产物依赖与资源隔离，不并行清理/重建同一输出目录，不用旧产物补成通过。

## 构建与部署

- 创建默认包含开发期部署工具，可用 --no-deploy 关闭。每个镜像使用 images/ 中独立 Dockerfile，Web 配置位于 images/web/，不使用镜像阶段 target 配置。
- 构建器生成 images/dependencies/ 安装输入；fetch、安装和生产依赖整理共用 pnpm store，安装与生产依赖整理共用 SQLite 驱动缓存，并复用依赖层。file: 归档、补丁、workspace 元数据必须进入下载上下文；原生依赖缓存按系统/架构/ABI 区分。基础镜像、中间件及工具指定版本，禁止 latest。
- 默认容器面向 CPU 运行或远程 AI 服务，Dockerfile 设置 `ONNXRUNTIME_NODE_INSTALL_CUDA=skip`，跳过额外 CUDA/TensorRT 下载；使用宿主机 GPU 时定制镜像和运行参数，不修改本机 npm 或全局 AI/GPU 配置。
- pnpm run deploy --build 显式串联构建、打包和发布；--bundle-only 只交付完整离线包与跨平台脚本；--services 以服务器成功状态为基线只更新选中服务。完整包自动包含自建中间件镜像，服务器禁止联网拉取。
- `pnpm build` 默认仅编译源码（调用 compile）；`pnpm build --target docker` 根据 hai.build.json 的 dockerfile 构建并导出 app/可选 web 镜像，可用 --images web 只构建前端，auto 优先可达 Podman、其次 Docker，可用 --runtime 显式指定。
- compile 交付源码编译结果，docker 交付镜像归档，desktop 交付 Tauri 系统安装包；每次只选一种。images[].dockerfile 指向独立镜像文件，最终阶段作为交付镜像；参数默认值和示例必须与真实入口同步。
- Podman 的客户端代理通过预定义构建参数传递，覆盖 Windows/macOS 远程引擎的服务端默认值；代理地址必须能从构建容器访问，--no-http-proxy 禁止此传递。不要把代理写成 Dockerfile ENV 或固化进交付镜像。
- --network 默认沿用引擎网络；只有实际构建网络无法访问依赖/代理时才显式选择网络，pasta 仅适用于 rootless Podman。build 与 run 的主机名解析可能不同，须验证构建网络中的可达性，不默认改成 host 网络。
- 从运行 build 的应用根 package.json.version 读取版本，不能使用 CLI/框架版本或服务子包版本代替。清单格式 version=3，appVersion 为应用版本；appContainerPort/webContainerPort 仅记录 images 配置显式填写的端口，部分构建也保留未选镜像端口，未填写时沿用部署默认值。镜像 tar 名含版本、buildId 区分同版本构建，Tauri 也接收根版本。不要手改清单或保留无版本的交付归档。
- 构建不区分 dev/test/prod，不加载部署环境覆盖，不注入运行期凭据；所选镜像清单含 SHA-256，全部成功才更新本机最近构建指针（不是镜像标签）。不要让 compile、Dockerfile 或 Tauri beforeBuildCommand 递归调用自身的原生/镜像构建。
- Vite 的 envDir 与 SvelteKit 的 kit.env.dir 是独立加载入口，必须同时阻止构建阶段读取工程根的 .env 文件；开发/预览由 core loader 注入，运行期由 deploy 注入。用临时 .env.local 哨兵值检查输出，避免只过滤进程变量却仍把私有文件固化进产物。
- 静态客户端通过 runtime-config.js 或原生宿主初始化脚本获取公开地址与元数据；仅投影公开白名单，禁止将完整 process.env、服务配置或密钥序列化到浏览器。预览与容器启动均须验证真实登录，不能依赖构建期 API 地址让 E2E 通过。
- Fullstack Capacitor App 在 `public/runtime-config.js` 提供公开 API 地址，再编译并同步原生资源；Tauri 从启动进程的 PUBLIC_API_BASE 注入。不能把开发代理的 .env 配置当作已安装原生包的运行配置。
- `pnpm build --target desktop --platform windows|linux|macos --arch x64|arm64` 调用对应工程的 Tauri build。要求在目标系统准备 Rust target、SDK 和打包工具，不假定跨系统编译。原生安装包必须按目标系统实测。
- deploy 默认消费已有产物，--build 显式先构建，详见 [hai-deploy](../hai-deploy/SKILL.md)。网络不通时 --bundle-only 生成 tar，按说明手工搬运；packaged 不代表部署已运行。
- 测试临时工程、镜像、容器、辅助进程和数据必须在 finally/teardown 自动清理，失败/取消也清理；日志、截图和报告保留到分析和交付结束再清理，不得提交测试产物或删除用户既有资源。
- 浏览器测试的数据放入本轮独立临时目录；Windows 等子进程与服务退出后再删除数据库和上传文件，不依赖下一轮测试补清理。

## 按需参考

按任务读取 [reference.md](reference.md) 的对应小节；签名以当前源码或安装版本的公开类型为准，不默认加载全部资料。示例中的业务变量由应用提供。

- 适用场景
- 项目架构
- 技能导航
- 行为开发工作流
- 统一编码规范
- 标准工作流
