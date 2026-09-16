# ZannenToolbox

面向 Zannen 系列硬件（ZannenSmol / ZannenSmolAir / ZannenDongle）的模块化桌面工具箱。

- **轻壳 + 插件**：类似 nRF Connect 的模块化功能装载；插件 = 原生动态库（C ABI）+ 独立打包的前端 ESM，运行时动态装载
- **现代 UI**：扁平化 × Material 层级为底，玻璃拟态表面，微交互动效（深色优先）
- **轻量高效**：Tauri 2（系统 WebView，无 Chromium 打包）+ Rust 后端；安装包预算 ≤ 25MB
- **跨平台**：Windows x86-64（NSIS）/ macOS ARM+Intel（universal2 DMG）
- **品牌化安装与更新**：定制安装器视觉、全新/升级安装识别与首启动效、本体（tauri-plugin-updater）与插件（自研管线）双轨在线更新，更新源占位待启用

内置调试模块（`zannen.debugger`）覆盖：硬件自动识别（串口双协议探测 + USB VID/PID/Product 指纹 + BLE 广播名，与 SlimeVR-Tracker-nRF 系真实固件对齐）、UF2 固件刷写与升级、3D 运动渲染、传感器/RF 原始数据时间轴、串口命令行终端、硬件状态自动解读。

## 快速开始

前置：Rust stable（rustup）、Node ≥ 20、pnpm；macOS 需 Xcode CLT。

```sh
pnpm install          # 安装前端依赖
pnpm dev              # 装配调试插件并启动开发模式（含 mock 虚拟设备，无硬件可体验）
```

常用命令：

```sh
pnpm test             # Rust 全工作区测试
pnpm build:plugins    # 只装配插件（app-shell/src-tauri/plugins/）
pnpm build            # 前端 + Rust 全量构建
pnpm size-check       # 产物体积报告（对照预算）
pnpm tauri build      # 打安装包（在 app-shell 目录下执行）
```

无硬件演示：开发构建自带 `mock-transport`，扫描设备可发现 `mock://zannen-*` 虚拟设备，提供完整的 IMU/RF/status 数据流与命令交互。

## 仓库结构

```
app-shell/        # Tauri 壳（前端 React + src-tauri Rust）
crates/
  zannen-plugin-api/   # 插件 ABI 契约（宿主与插件共用）
  zannen-core/         # 插件管理器、串口/BLE/UF2 服务、事件总线、mock 传输层
packages/
  plugin-sdk/          # 插件前端 SDK（IPC、hooks、玻璃拟态组件）
plugins/
  zannen-debugger/     # 调试模块（参考插件）：后端 cdylib + 前端六视图
scripts/          # 装配 / 开发 / 体积检查脚本
docs/             # 架构、插件 ABI、调试模块、UI 规范
```

## 文档

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 总体架构、数据通道、性能与体积预算、构建矩阵、插件信任模型
- [docs/PLUGIN-ABI.md](docs/PLUGIN-ABI.md) — 插件 ABI 契约、宿主服务目录、清单格式、新插件上手指南
- [docs/MODULE-DEBUGGER.md](docs/MODULE-DEBUGGER.md) — 调试模块：识别流程、数据协议、UF2 流程、状态规则引擎、mock 设备
- [docs/UI-DESIGN.md](docs/UI-DESIGN.md) — 设计令牌、组件规范、动效、可访问性、性能规则
- [docs/PACKAGING-UPDATES.md](docs/PACKAGING-UPDATES.md) — 品牌化安装包、全新/升级识别、本体与插件在线更新、密钥与 CI
- [docs/ERROR-CODES.md](docs/ERROR-CODES.md) — 错误识别码目录（报错前缀 `[Exxxx]` 速查）

## 状态与路线

当前为 **v0.4.x**（见 [CHANGELOG.md](CHANGELOG.md)）：BLE 数据面、MCUboot/SMP 串行 DFU、热插拔、i18n、插件签名、CI 流水线、单窗口层级导航、原生窗口动画、品牌化安装包与双轨在线更新、开机自启动、更新日志弹窗均已落地。后续候选见 docs/ARCHITECTURE.md §9（插件市场目录、BLE 上的 SMP、二进制有线协议等）。
