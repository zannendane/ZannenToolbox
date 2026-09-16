# ZannenToolbox 总体架构

> 版本：v0.4.x · 平台：Windows 10/11 x86-64、macOS（Apple Silicon / Intel）

## 1. 定位与目标

ZannenToolbox 是面向 Zannen 系列硬件（ZannenSmol / ZannenSmolAir 跟踪器、ZannenDongle 接收器）的桌面工具箱。核心特性：

- **模块化功能装载**：类似 nRF Connect 生态的"轻壳 + 功能模块"模式，模块（插件）在运行时动态装载/卸载，第三方可独立开发；
- **现代 UI**：扁平化 + Material 层级为底，玻璃拟态表面，微交互动效；
- **性能与体积**：安装包 ≤ 25MB，冷启动 ≤ 1.5s，空闲内存 ≤ 150MB，数据可视化 60fps；
- **跨平台**：单代码库产出 Windows x64（NSIS）与 macOS universal2（DMG）安装包。

## 2. 技术栈与关键决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 应用框架 | Tauri 2 | 复用系统 WebView（WKWebView / WebView2），安装包 ~1/10 于 Electron；Rust 后端直接承载硬件 IO |
| 前端 | React 19 + TypeScript + Vite | 生态成熟；壳与插件共享同一 React 实例（vendor chunk），避免多实例问题 |
| 插件形态 | 原生动态库（.dll/.dylib/.so，C ABI）+ 独立打包的 ESM 前端 | 性能最佳、隔离性好；ABI 稳定边界明确 |
| 图表 | uPlot（canvas） | ~45KB，百万级点 60fps；远轻于 echarts |
| 3D | three.js | 标准选择；按需注册，后续可换自研 WebGL 渲染器进一步减重 |
| 串口 | `serialport` crate | 跨平台成熟 |
| BLE | `btleplug` crate | 跨平台 BLE central |
| UF2 | 自研 codec（512B 块解析）+ 卷检测（`sysinfo`） | 格式简单，无第三方依赖 |
| 状态管理 | zustand（壳）/ 插件内自建 store | 轻量 |
| 动效 | framer-motion | 微交互与布局过渡；`prefers-reduced-motion` 全局降级 |
| 路由 | 无路由库 | 壳导航即"插件选择器"，自绘 ~50 行，省 ~60KB |

### 为什么不选 Electron / Qt / 纯 Rust GUI

- **Electron**：Chromium + Node 打包导致安装包 150MB+、常驻内存 300MB+，与"极简轻量化"目标冲突；
- **Qt/QML**：性能优秀，但 LGPL 动态链接合规/商业授权成本高，QML 对玻璃拟态等 CSS 级视觉表达力弱于 Web 技术栈；
- **egui/iced**：二进制最小，但复杂图表、玻璃模糊、3D 均需自建，调试模块交付周期不可控。

## 3. 进程内架构

```
┌──────────────────────────── app-shell ────────────────────────────┐
│ 前端（React，壳）                                                  │
│  ├─ LauncherView（模块选择主页）/ PluginTabs（返回上级 + 路由标签）       │
│  ├─ TopBar（面包屑 + 插件控件插槽 .topbar-slot）/ FloatingBar（外观/语言/设置）│
│  ├─ pluginLoader：拉取插件 ESM → 重写裸导入 specifier → blob import │
│  └─ bus.ts：zannen-bus 事件 → 设备注册表状态                       │
│ Rust（zannen-toolbox bin）                                         │
│  ├─ Tauri 命令：plugin_list / plugin_invoke / shell_info           │
│  ├─ plugin-asset:// 协议：插件目录静态资源（路径逃逸防护）           │
│  └─ 事件转发器：bus → 前端 emit；serial.*/device.*/ble.* 回投插件   │
├────────────────────── zannen-core（lib）──────────────────────────┤
│ PluginManager：扫描 plugin.toml → libloading 加载 → ABI 握手 →      │
│   清单交叉校验 → invoke 路由（插件级 Mutex 串行化 + catch_unwind）   │
│ ServiceDispatcher（host_call 的落点）：                            │
│   serial / ble / uf2 / devices / events / log                      │
│ EventBus：tokio broadcast（1024 容量，慢消费者丢旧事件）             │
│ mock-transport（feature）：mock://zannen-* 虚拟设备                 │
└────────────────────────────────────────────────────────────────────┘
        ▲ C ABI（zannen-plugin-api）：版本握手 + JSON invoke + host_call vtable
┌───────┴────────────────────────────────────────────────────────────┐
│ plugins/zannen.debugger（cdylib + frontend/dist 单文件 ESM）         │
└────────────────────────────────────────────────────────────────────┘
```

### 目录即契约

```
app-shell/src-tauri/plugins/<plugin-id>/   # 出厂内置（打包进安装包资源，只读）
├── plugin.toml                 # 清单（与 dylib 导出清单交叉校验）
├── lib<backend>.dylib | <backend>.dll | lib<backend>.so
├── frontend/dist/index.js      # 单文件 ESM（外部依赖由壳 vendor 提供）
├── frontend/dist/style.css
└── *.json                      # 插件自带数据（指纹表、规则表…）

<app_data>/plugins/<plugin-id>/       # 用户目录（可写）：在线更新落点，同 id 覆盖内置版
```

内置目录解析顺序：环境变量 `ZANNEN_PLUGIN_DIR` → 可执行文件旁 `plugins/` → macOS bundle `Contents/Resources/plugins/` → 开发态 `src-tauri/plugins/`。装载时先内置后用户目录，同 id 后者覆盖（shadow）。`plugin-asset://` 协议同样按此优先级取文件。

## 4. 插件前端装载机制（关键设计）

裸导入 specifier 不能出现在运行时动态加载的 ESM 里。实现采用 **specifier 重写 + Blob import**：

1. `plugin-asset://localhost/<plugin-id>/frontend/dist/index.js` 拉取插件源码文本；
2. 正则重写 `react` / `react/jsx-runtime` / `@zannen/plugin-sdk` / `@tauri-apps/api/*` / `framer-motion` 等 specifier 为壳的 vendor **完整 URL**（含 origin；host 相对路径在 blob: 模块内无法被 WKWebView 解析）；
3. `Blob` → `URL.createObjectURL` → `import()`；失败回退为错误占位视图，壳不受影响；
4. 插件 CSS 经 `<link>` 注入一次（清单注册时即注入，不等待模块装载）。

**为什么不用 import map**：WKWebView 需 Safari 16.4+（macOS 13.3+），且 dev/prod 的 vendor URL 不一致需要动态 import map（兼容性更差）。blob import 兼容面宽、错误可精确降级。**为什么 vendor 共享而不是各自打包**：react 等单例库重复实例会触发 Invalid hook call；vendor chunk 经 rollup code splitting 与壳主包共享同一模块实例（`preserveEntrySignatures: "strict"` + `minifyInternalExports: false`，保证具名导出对插件可见）。无单例要求的库（lucide-react、three、uplot）由各插件自行打包并 tree-shake。

### 装载性能

- 空闲（`requestIdleCallback`，老 WebView 退化为短延时）按"当前选中优先"后台预装载全部插件模块——首次点击零等待；
- 懒装载仍是底线：预热失败/未完成的插件在选中时才装载，互不阻塞启动；
- 已评估的进一步拆分路线（暂未启用）：插件前端按视图分包（three.js 仅在打开 3D 视图时解析），需把包内相对 chunk 引用重写为 plugin-asset 完整 URL（含静态跨 chunk 引用），blob→自定义协议的跨源模块加载在 WKWebView 的可靠性需真机验证后再启用。

## 5. 数据通道

| 通道 | 方向 | 用途 | 节流策略 |
|---|---|---|---|
| `plugin_invoke` 命令 | 前端 → 插件 | 请求/响应（扫描、写串口、刷写触发） | — |
| 插件 `host_call` | 插件 → 宿主服务 | 串口/BLE/UF2/设备注册表/事件上行 | 同步 JSON |
| `zannen-bus` 事件 | 宿主 → 前端 | 全部总线事件透传 | 插件侧对高频数据按 33ms 批量（`imu.batch`/`rf.batch`）；`serial.rx` 按读块原样转发（前端 100ms 合帧渲染） |
| 总线回投 | 宿主 → 插件 | `serial.*` / `device.*` / `ble.*` 供插件解析 | 仅硬件主题回投，插件自身 emit 的主题不回投（防回声循环） |

## 6. 性能与体积预算

| 指标 | 预算 | 保障手段 |
|---|---|---|
| 安装包 | ≤ 25MB | Tauri（无 Chromium 打包）；release: `lto=fat` + `codegen-units=1` + `strip` + `opt-level="z"` |
| 冷启动 | ≤ 1.5s（M 系 Mac / 中端 Windows） | 壳前端 ≤ 300KB gzip；插件清单同步扫描，dylib 加载无网络 |
| 空闲内存 | ≤ 150MB | 单 WebView；无后台轮询（BLE 扫描仅按需） |
| 数据渲染 | 60fps @ IMU 100Hz | uPlot canvas + rAF；33ms 批量推送；前端缓冲上限（丢旧保新） |
| 前端 gzip | 壳 ≤ 300KB；插件 ≤ 1.5MB | `scripts/size-check.sh` 报告；CI 阈值告警（roadmap） |

## 7. 跨平台构建矩阵

| 目标 | 工具链 | 产物 |
|---|---|---|
| macOS arm64 + x86_64 | 本机 `rustup target add aarch64-apple-darwin x86_64-apple-darwin`，`tauri build --target universal2-apple-darwin` | 单 DMG（universal2） |
| Windows x86_64 | Windows CI runner，`tauri build` | NSIS 安装包 |
| 插件动态库 | 随各 target 分别编译 | 插件目录内多平台文件并存（按平台名区分），或按平台分包发布（roadmap） |

注意：macOS 上 BLE 需要用户在系统弹窗中授予蓝牙权限；`src-tauri/Info.plist` 已声明 `NSBluetoothAlwaysUsageDescription`（缺失会直接 TCC 杀进程闪退，不是弹窗——这是 macOS 的硬约束）。

## 8. 插件信任模型（现状与路线）

原生动态库拥有进程级权限——在线安装路径已由 ed25519 签名守护（壳内公钥固定，未签名默认拒绝，见 docs/PACKAGING-UPDATES.md §4）；手动放入 plugins 目录的插件当前仍假定可信。路线图：

1. 插件签名（ed25519）+ 壳内公钥固定，未签名插件加载时显式警告；
2. 能力声明（manifest.capabilities）与宿主服务调用的运行时审计；
3. 可选：纯前端插件沙箱（无后端动态库）用于低风险 UI 扩展。

## 9. 版本治理

版本号分层管理，**壳与插件解耦**（`scripts/bump-version.sh` 统一递增）：

| 层 | 版本载体 | 递增时机 | 用途 |
|---|---|---|---|
| 壳（产品） | `tauri.conf.json`、`app-shell/package.json`、`src-tauri/Cargo.toml`、根 `package.json` | **仅壳自身代码/资源变化时**（不含出厂插件内容变化——插件有独立版本表达） | 本体在线更新、DMG/NSIS 文件名、首启/升级欢迎流、`shell_info` |
| 各插件（独立） | `plugins/<name>/{Cargo.toml,plugin.toml,frontend/package.json}` + 装配副本 | **仅该插件内容变化时** | 插件在线更新比较（SemVer 严格更大才更新）、dylib↔plugin.toml 交叉校验、UI 展示 |
| 内部 crate / SDK | `crates/*/Cargo.toml`、`packages/plugin-sdk/package.json` | 随壳锁步（内部实现细节，无外部消费者） | — |
| ABI 契约 | `ZANNEN_ABI_VERSION`（整数常量） | ABI 不兼容变更时 +1 | 插件加载握手（不符拒载） |

配套规则：

- **遮蔽语义**：用户目录插件（在线更新落点）仅当版本**严格高于**出厂同 id 版本才遮蔽；相等或更低时出厂版生效，陈旧用户插件被忽略（`should_shadow`，避免旧用户插件永久遮蔽新出厂插件）；
- 在线更新安装相同版本（重装）不遮蔽出厂版；
- **分发通道**：插件内容变化经插件包（.znplugin）与插件在线更新通道分发；本地重新部署应用包不强制壳版本步进。出厂插件内容更新但壳未变时，壳版本保持不动（升级欢迎流不触发，属预期——变更在插件版本上表达）；
- CHANGELOG 按壳（产品）版本记录；插件独立版本在插件更新通道使用。

## 10. 路线图

已完成：~~BLE 数据面~~、~~MCUboot/SMP 串行 DFU~~、~~插件前端懒装载~~、~~插件远程更新（含 ed25519 签名验签）~~、~~串口热插拔~~、~~多设备分轨~~、~~i18n~~、~~CI/release 流水线~~（0.2.0）；~~单窗口层级导航与模块主页~~、~~原生窗口几何动画~~、~~品牌化安装包与本体在线更新~~、~~标题本地化桥~~、~~开机自启动~~、~~更新日志弹窗~~（0.3.x–0.4.x，均见 CHANGELOG）。

后续候选：
- 插件市场式目录（远程索引 + 签名强制）
- 硬件协议正式化（JSON Lines → 带 CRC 的二进制帧，协议层在插件内可替换）
- Windows 自绘标题栏的 Snap Layout 悬停支持
- BLE 上的 SMP DFU（mcumgr over BLE）
- Icon Composer `.icon`（macOS 26 系统级图标变体）
