# UI 设计规范

> 基调：**扁平化 × Material 层级**为底，**玻璃拟态**用于浮层与卡片，**微交互**提供操作反馈。
> 实现：设计令牌集中在 `app-shell/src/styles/tokens.css`；组件类 `zt-*` 在 `components.css`，壳与全部插件共用。

## 1. 设计原则

1. **内容优先**：装饰让位数据；玻璃模糊只用于承载层（卡片/菜单/弹层），不用于纯装饰；
2. **层级用明度，不用投影堆砌**：背景三层（`--zt-bg-0/1/2`）明度递进；投影克制（`--zt-shadow-1/2`）；
3. **动效即反馈**：所有动效回答"发生了什么"（hover 凸起、press 下沉、路由滑动、开关弹跳），无纯观赏动画；
4. **深色优先**：硬件调试工具的主战场是低光环境；令牌化保证后续加浅色主题时零改组件。

## 2. 设计令牌（节选）

| 类别 | 令牌 | 值 |
|---|---|---|
| 背景 | `--zt-bg-0/1/2` | `#0b0d13` / `#10131b` / `#161a24` |
| 玻璃 | `--zt-glass-bg` / `--zt-glass-border` / `--zt-glass-blur` | `rgba(255,255,255,.055)` / `.1` / `18px` |
| 文本 | `--zt-text-1/2/3` | 白 93% / 62% / 38% |
| 强调 | `--zt-accent` → `--zt-accent-2` | `#6e8bff` → `#9b7bff`（135° 渐变仅用于主按钮/激活态） |
| 语义 | `--zt-ok` / `--zt-warn` / `--zt-critical` | `#34c77b` / `#f5a524` / `#f0474a`（各配 14% 透明 soft 底） |
| 圆角 | `--zt-radius-s/m/l` | 8 / 12 / 16px |
| 字体 | 系统栈（SF Pro/Segoe UI/苹方/雅黑），等宽 `--zt-font-mono` | 零下载，原生观感 |
| 动效 | `--zt-ease-out: cubic-bezier(.22,1,.36,1)`；快 120ms / 中 240ms | — |

## 3. 组件规范（`zt-*`）

| 组件 | 类 | 规范 |
|---|---|---|
| 玻璃卡片 | `.zt-card` | blur 18px + 1px 半透明描边 + 内顶部 1px 高光；入场 fade+8px 上浮 |
| 按钮 | `.zt-btn-{primary,ghost,danger}` | 主按钮渐变；ghost 玻璃底；hover 1.03 / press 0.96 弹簧缩放（stiffness 500/damping 28） |
| 徽标 | `.zt-badge-{ok,warn,critical,accent,muted}` | 语义色 soft 底 + 同色系描边；状态只用语义色，不用装饰色 |
| 开关 | `.zt-toggle` | 滑块弹簧位移动画；`role="switch"` + `aria-checked` |
| 进度 | `.zt-progress` | determinate 渐变条；indeterminate 往返动画 |
| 分段 | `.zt-segmented` | 玻璃槽 + 激活块投影 |
| 空状态 | `.zt-empty` | 图标 + 标题 + 一句行动指引（永远告诉用户下一步） |

## 4. 玻璃拟态使用规则

- **可以**：卡片、下拉菜单、终端容器、3D 视口容器；
- **不可以**：正文文本背景（可读性）、密集列表行（性能）、小尺寸按钮（模糊无意义）；
- 依赖背景氛围：`body::before` 的两团径向光晕是玻璃模糊的"内容源"，改背景时必须保留可透内容。

## 5. 微交互与动效

| 场景 | 动效 | 参数 |
|---|---|---|
| 路由切换 | fade + 横向 14px 滑动 | 180ms ease-out，mode="wait" |
| 侧边栏激活 | 左侧 3px 指示条 layoutId 滑动 | spring 480/38 |
| 菜单弹出 | fade + -6px → 0 + scale .98→1 | 160ms |
| 按钮 | hover/press 缩放 | spring 500/28 |
| 卡片入场 | opacity 0→1，y 8→0 | 250ms `--zt-ease-out` |

**降级**：`prefers-reduced-motion: reduce` 时全局时长归零（tokens.css + base.css 双层兜底）。

## 6. 可访问性

- 焦点环：`:focus-visible` 2px 强调色描边，全组件生效；
- 对比度：`--zt-text-1/2` 对背景对比度 > 7:1 / 4.5:1；语义色文本仅用于 soft 底上；
- 开关/按钮 ARIA 角色齐备；终端输出区 `user-select: text` 可复制；
- 动效降级见 §5。

## 7. 性能规则（写新视图前必读）

1. 高频数据（>10Hz）**禁止**直接进 React state——缓冲到 ref/外部数组，rAF 或 100ms 合帧；
2. 图表只用 canvas 方案（uPlot）；禁止 SVG 逐点渲染；
3. 图标用 lucide 按需 import（插件内 tree-shake），禁止 `icons` 全量命名空间；
4. 新依赖先过体积关：gzip > 50KB 需在 PR 说明理由（`scripts/size-check.sh` 对照）；
5. 玻璃模糊面积克制：同屏 blur 区域 ≤ 视口 60%（backdrop-filter 是 GPU 大户）。

## 8. 平台差异

| | macOS | Windows |
|---|---|---|
| 标题栏 | `titleBarStyle: Overlay` 原生红绿灯 + 拖拽区 | 无边框 + 自绘 min/max/close（`tauri.windows.conf.json`） |
| WebView | WKWebView | WebView2（Evergreen） |
| 自定义协议 | `plugin-asset://localhost` | `http://plugin-asset.localhost`（壳自动适配） |

## 9. 深浅色主题系统

- **模式**：`auto`（跟随系统）/ `light` / `dark`，持久化在 localStorage（`zannen-theme`），`public/theme-init.js` 在模块加载前落 `data-theme`，零闪烁；
- **跟随系统**：auto 模式监听 `prefers-color-scheme` 变化实时切换；
- **手动切换**：顶栏右侧按钮循环 auto→light→dark；
- **壳与插件同步**：所有令牌挂 `:root[data-theme=…]`，插件与壳同一文档 → 天然同步；插件内硬编码画布（uPlot/three.js）经 SDK `useTheme()` 订阅 `zannen-theme` 事件重建配色；
- **原生侧同步**：`set_window_theme`（窗口边框/对话框）+ `set_app_icon`（Windows 任务栏窗口图标 + macOS Dock 图标 `NSApplication.setApplicationIconImage`）；
- **图标资产**：暗/亮双版源图（`branding/`）→ `scripts/gen-themed-icons.swift` 生成 1024 母版（打包）与 512 运行时版（内嵌二进制）；安装包图标为叠加 📦 角标的变体（`installer/installer.ico`）；
- **NSIS 安装器**：Win32 经典 UI 无法整体换肤，构建期以品牌深色侧栏统一呈现；hooks 检测系统外观写入安装日志。DMG 由 Finder 呈现，天然跟随系统外观。
