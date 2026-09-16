# 打包、安装与更新

> 覆盖：品牌化安装包（NSIS / DMG）、全新安装与升级识别、本体在线更新、插件在线更新、密钥与 CI。

## 1. 打包产物

| 平台 | 产物 | 说明 |
|---|---|---|
| Windows x86-64 | `*-setup.exe`（NSIS） | 品牌化安装向导：自定义侧栏/页眉/图标，中英文语言选择，lzma 压缩 |
| macOS universal2 | `.dmg` | 品牌化拖放窗口（自定义背景 + 图标定位），单 DMG 同时含 ARM/Intel |

构建：

```sh
# 视觉资产（程序化生成，可随时重生成，不入库大文件）
node scripts/gen-installer-assets.mjs

cd app-shell
pnpm tauri build                          # 当前平台
pnpm tauri build --target universal2-apple-darwin          # macOS universal2
pnpm tauri build --target x86_64-pc-windows-msvc           # Windows（需在 Windows/CI）
```

安装器视觉资产（`src-tauri/installer/`）由脚本生成：`sidebar.bmp`(164×314)、`header.bmp`(150×57)、`dmg-background.png`(660×400)。改品牌色只需改脚本里的调色常量再跑一遍。

### 图标资产（主题双版）

```sh
pnpm build:assets   # 安装器视觉资产 + 主题图标全链路（重生成并同步到 src-tauri）
```

- 源图：`branding/ZannenToolbox图标-{暗色,亮色}模式.png`（2048²，含矢量 SVG 参考）
- `scripts/gen-themed-icons.swift`（AppKit 合成）产出：
  - `icon-dark.png` / `icon-light.png`（1024² 母版；暗色版为打包静态图标）
  - `icon-dark-512.png` / `icon-light-512.png`（运行时切换用，内嵌二进制）
  - `installer-dark.png` / `installer-light.png`（右下角 📦 角标的安装包变体 → `installer/installer.ico` 供 NSIS；角标为仿 📦 emoji 的纸板箱：顶盖折面 + 中央胶带 + 货运标签）
- **Windows 安装包统一暗色**：installer.ico 固定使用暗色变体，侧栏/页眉为品牌深色；NSIS 是 Win32 经典控件体系，向导正文区不支持深色换肤（hooks 会检测系统外观写入安装日志）
- 运行时代价：两张 512² PNG 内嵌约 +360KB；Dock/任务栏图标随主题切换即时生效

### 安装器配置键（tauri.conf.json）

```jsonc
{
  "bundle": {
    "createUpdaterArtifacts": true,          // 额外产出签名更新包
    "windows": {
      "nsis": {
        "installerIcon": "icons/icon.ico",
        "headerImage": "installer/header.bmp",
        "sidebarImage": "installer/sidebar.bmp",
        "installMode": "both",               // 当前用户（免管理员）/ 全机
        "languages": ["zh-CN", "en-US"],
        "displayLanguageSelector": true,
        "compression": "lzma",
        "installerHooks": "installer/hooks.nsh",
        "startMenuFolder": "ZannenToolbox"
      }
    },
    "macOS": {
      "dmg": {
        "background": "installer/dmg-background.png",
        "windowSize": { "width": 660, "height": 400 },
        "appPosition": { "x": 180, "y": 190 },
        "applicationFolderPosition": { "x": 480, "y": 190 }
      }
    }
  }
}
```

## 2. 全新安装 vs 升级识别

双通道：

1. **安装器层（Windows）**：`installer/hooks.nsh` 的 `NSIS_HOOK_PREINIT` 读注册表卸载项（HKCU/HKLM `...\Uninstall\ZannenToolbox` 的 `DisplayVersion`）判定 fresh/upgrade，`DetailPrint` 记录；`POSTINSTALL` 把 `{"install_kind","prev_version"}` 写入 `$INSTDIR\.zannen-install-state`（诊断与审计用）。Tauri 模板在升级时自动卸载旧版，数据目录不受影响。
2. **应用层（全平台，用户可见）**：首启读取 `app_data/zannen-state.json` 的 `last_run_version`：
   - 无记录 → **全新安装引导页**（品牌入场 + 功能亮点分步浮现）
   - 版本不同 → **升级欢迎页**（版本号滚动过渡 + 更新说明 + 插件兼容性提示）
   - 相同 → 直接进入工作台
   展示后写回当前版本。macOS 的 DMG 覆盖安装与 NSIS 升级走同一判定，行为一致。

## 3. 本体在线更新（tauri-plugin-updater）

- 配置：`plugins.updater.endpoints`（当前为占位 `https://updates.zannen.dev/toolbox/{{target}}/{{current_version}}`）+ `pubkey`（minisign 公钥）
- 行为：启动后 3s 静默检查；有更新 → 侧边栏「更新」出现脉冲提示点 → 更新中心展示 release notes → 下载（进度条）→ 重启生效
- 离线/未配置：检查静默跳过，不产生任何弹窗或报错
- 更新服务应答格式（Tauri 约定）：

```json
{
  "version": "0.2.0",
  "notes": "更新说明",
  "pub_date": "2026-09-04T00:00:00Z",
  "platforms": {
    "darwin-aarch64": { "url": "https://…/ZannenToolbox.app.tar.gz", "signature": "…" },
    "windows-x86_64": { "url": "https://…/ZannenToolbox-setup.exe", "signature": "…" }
  }
}
```

`tauri build` 在设置签名私钥后自动产出更新包与 `.sig`（`target/release/bundle/…`）。

### 密钥管理

- 开发密钥对已生成于 `~/.tauri/zannen-toolbox.key`（私钥）/`.pub`（公钥，已写入 tauri.conf.json）
- **CI / 正式发布**：私钥放入 CI secret，构建时设环境变量：
  - `TAURI_SIGNING_PRIVATE_KEY`（私钥内容或 `TAURI_SIGNING_PRIVATE_KEY_PATH`）
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- 私钥丢失 = 永远失去推送更新的能力，务必多备份
- 分发正式版时另需：macOS 开发者签名 + 公证（`APPLE_CERTIFICATE` 等）、Windows 代码签名证书，见 Tauri 官方文档

## 4. 插件在线更新（自研管线）

### 更新源

内置默认 `src-tauri/update-sources.json`：插件源指向**本仓库滚动 Release `plugins-latest`**（公开仓库匿名可下载，由 `.github/workflows/plugins.yml` 发布）；运行时可在应用数据目录放同名文件覆盖：

```json
{
  "app": { "enabled": true, "manifest": "https://…/toolbox/manifest.json" },
  "plugins": {
    "zannen.debugger": { "enabled": true, "manifest": "https://github.com/zannendane/ZannenToolbox/releases/download/plugins-latest/manifest-zannen.debugger.json" }
  }
}
```

### 插件分发 CI（plugins.yml）

- 触发：`plugins-v*` 标签或手动 workflow_dispatch；
- 矩阵构建：macos-14（aarch64）/ macos-13（x86_64）/ windows-2022（x86_64），逐平台构建并签名（`PLUGIN_ED25519_PRIVATE_KEY` secret 写入 `~/.zannen/keys/` 供 sign-plugin.mjs 读取）；
- 逐平台生成清单片段（`scripts/gen-plugin-manifest.mjs`），publish 汇总合并（`scripts/merge-plugin-manifests.mjs`）；
- 发布到本仓库滚动 Release `plugins-latest`：`.znplugin` + `.sig` + `manifest-<id>.json`，同名资产覆盖更新；壳更新中心一键检查 → 下载 → 验签 → 原子安装 → 热重载。

### 插件更新清单（服务端应答）

```json
{
  "version": "0.2.0",
  "min_api": 1,
  "notes": "新增 …",
  "files": {
    "macos-aarch64":  { "url": "https://…/zannen.debugger-0.2.0-macos-aarch64.znplugin", "sha256": "…", "signature": "…" },
    "macos-x86_64":   { "url": "https://…/zannen.debugger-0.2.0-macos-x86_64.znplugin",  "sha256": "…", "signature": "…" },
    "windows-x86_64": { "url": "https://…/zannen.debugger-0.2.0-windows-x86_64.znplugin", "sha256": "…", "signature": "…" }
  }
}
```

`signature` 为对 .znplugin 完整字节的 ed25519 签名（hex），由 `scripts/sign-plugin.mjs` 产出（同 `.sig` 文件内容）。

`target` 键由 `shell_info.target`（`{os}-{arch}`，如 `macos-aarch64`）匹配。

### 插件包格式（.znplugin = zip）

```
plugin.toml                  # 必需；id 与目标一致，api ≤ 宿主 ABI
lib<name>.dylib / <name>.dll / lib<name>.so   # 至少含当前平台
frontend/dist/index.js       # 清单声明前端时必需
frontend/dist/style.css
（其余数据文件原样）
```

### 安装管线（全部失败可回滚）

```
下载 → sha256 校验 → ed25519 验签（对包文件完整字节，壳内固定公钥）
→ zip 路径净化（拒 .. 与绝对路径）→ plugin.toml 校验
→ ABI 兼容检查 → staging 解压 → rename 原子替换（旧目录先转备份）
→ PluginManager.reload（destroy + 重新 dlopen）→ 前端模块重新装载
```

降级安装会给出警告但允许执行。插件更新会销毁其运行实例（串口会话随之断开），UI 需提示。

### 信任模型

信任链 = ed25519 签名 + sha256（清单来自 HTTPS）：

- 签名工具 `scripts/sign-plugin.mjs`（Node 内建 crypto）：目录打包为 zip 后对**包文件完整字节**签名，产出 `.znplugin` + `.znplugin.sig`（hex）及一行清单用 JSON（sha256/signature/bytes）
- 密钥：`~/.zannen/keys/plugin-ed25519.pem`（私钥，0600，勿入库）/ `.pub.pem`；验签公钥 hex 固定在壳内 `zannen_core::plugin_installer::PLUGIN_SIGNING_PUBKEY_HEX`，轮换密钥须随壳发布新版本
- 未签名包默认拒绝；仅当调用方显式 `allow_unsigned`（前端更新管线固定为 false）才放行并在报告中给出中文警告

## 5. 更新中心 UI

壳内建视图（左下角悬浮工具栈「设置 → 更新中心」）：本体卡片（版本/检查/下载进度/重启）、插件列表（逐插件检查/更新/状态徽标/失败原因）、更新源状态提示。离线或未配置时显示占位说明而非报错。

## 6. 离线安装承诺

- DMG / NSIS 安装包完全离线可用，不内嵌任何运行时下载
- 更新逻辑仅在更新源启用且网络可达时触发；任何失败静默降级
- mock 虚拟设备保证无硬件、无网络也能体验全部界面
