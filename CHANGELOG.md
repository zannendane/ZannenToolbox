# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.5.8] - 2026-09-11

### Added

- **目标语言直通**（zannen.translator 插件 → 0.5.0，壳版本不变）：识别语言与目标语言同族（忽略区域后缀）时跳过翻译请求直接输出原文——中间稿同义直出、断句免请求；历史与悬浮窗译文标记「原文直通」徽标

### Changed

- **翻译延迟再优化**（zannen.translator 插件 → 0.5.0，壳版本不变）：
  - 关闭思考模式：按端点自动注入——千问/DashScope 顶层 `enable_thinking: false`（官方实测总耗时降 60–75%）、DeepSeek `thinking: {"type":"disabled"}`；其余提供源不注入（防严格校验报错）
  - HTTP 连接池复用：共享 ureq agent（省每请求 TCP+TLS 握手，降低首 token 延迟）
  - 翻译 worker 并发池：final 断句 3 并发互不排队（乱序完成前端按 id 归位）、partial 中间稿独立单通道最新优先折叠；同传系统提示词精简（更少 prompt token）

### Added

- **悬浮窗独立任务栏项**：`overlay_open` 显式 `skip_taskbar(false)`，窗口标题改用插件显示名（任务栏/窗口列表可辨识，可独立从任务栏唤出）
- 悬浮窗左上角状态灯补齐**状态文字**（聆听中/处理中/错误等），与主窗口状态徽标同源（STATE_LABEL 共享）

### Changed

- **翻译全面改 SSE 流式**（zannen.translator 插件 → 0.4.0，DeepSeek 等 chat 提供源感知延迟大幅降低）：
  - openai-compatible 流式增量追加、dashscope（qwen-mt）兼容 flash/lite 增量与 plus/turbo 全量回放；mock/deepl 保持一次性
  - 逐 token 累计译文经 `partial_translation` 事件实时上屏（悬浮窗与主视图译文区同步滚动显示），最终译文到达后覆盖；实时逐词翻译与批量段译同路径受益

## [0.5.7] - 2026-09-09

### Added

- **翻译插件 v0.3.0**（2026-09-11，壳版本不变，插件独立分发）：
  - 悬浮窗/主视图指示灯四态：绿（有语音输入，随语音脉冲闪烁）/ 灰（静音）/ 红（错误）/ 黄（网络或性能告警，新错误码 E3605：提供源响应迟缓 >4s 非致命告警）；后端以 ~5Hz 上行音频电平与语音活动（带迟滞防抖动）
  - 实时逐词翻译与整体纠正：实时 STT 的中间稿经独立翻译 worker 滚动重译（partial_translation 事件，最新稿优先折叠积压），断句后最终译文覆盖；翻译移出 IO 线程不阻塞事件接收
  - 翻译流程与提示词优化：专业同传系统提示词（片段按原样翻译不补全）；近期 3 对译文作为上下文——openai-compatible 注入多轮消息、qwen-mt 注入 tm_list 翻译记忆

### Fixed

- **实时流式 STT 约 20 秒被服务端断开**（"request timeout"）：真正根因是 IO 架构缺陷——接收线程在互斥锁上阻塞 `read()` 时推流线程被饿死，音频帧基本发不出，服务端必然空闲超时。重构为单线程 IO 循环（底层流 50ms 读超时作 tick，同线程交替处理入站事件与出站通道，消除互斥锁）；并保留 `heartbeat: true` 与静音帧保活（zannen.translator 插件 → 0.2.3；壳版本不变，插件独立分发）

### Changed

- zannen.translator 插件版本号调整为 **v0.2.0**（按插件独立版本治理，版本线与壳解耦）
- 流程注记：本次壳版本步进（0.5.6→0.5.7）为流程失误——壳自身无变化时不应步进；规则已修正并固化（docs/ARCHITECTURE.md §9）：壳版本仅随壳自身变化递增，插件内容变化由插件独立版本表达，本地重新部署不强制壳步进。已部署版本不回退，后续按新规则执行

## [0.5.6] - 2026-09-09

### Fixed

- **WebSocket 实时 STT 连接握手失败**（`Missing, duplicated or incorrect header sec-websocket-key`）：握手请求改经 `IntoClientRequest` 由 URL 生成（自动生成 sec-websocket-key 等 WS 头）后再追加 Authorization 头——手工 `http::Request::builder` 构造的请求被 tungstenite 透传、缺握手头
- 版本递增脚本修复 macOS bash 3.2 兼容（多字节字符紧邻变量名的解析问题）与版本残留误报（精确匹配引号包裹的版本号）

### Changed

- zannen.translator 插件独立递增 0.5.4 → 0.5.5（WS 握手修复）；壳 0.5.5 → 0.5.6 搭载发布

## [0.5.5] - 2026-09-09

### Added

- **版本治理分层**（docs/ARCHITECTURE.md §9）：壳（产品）版本与各插件版本解耦——插件此后仅在自身内容变化时独立递增（`scripts/bump-version.sh shell|plugin` 统一递增，pnpm bump 可调用）；内部 crate/SDK 随壳锁步；ABI 版本为独立兼容契约
- 遮蔽语义修正：用户目录插件（在线更新落点）仅当版本**严格高于**出厂同 id 版本才遮蔽（`should_shadow` + `load_dir_if_newer`），相等或更低的陈旧用户插件不再永久遮蔽更新的出厂插件

### Changed

- 本版本起插件版本与壳解耦示范：壳 0.5.5，zannen.debugger / zannen.translator 保持 0.5.4（内容未变不递增）

## [0.5.4] - 2026-09-09

### Added

- 防误配防护：HTTP（OpenAI 兼容）STT 通道下模型名含 `-streaming` 时，配置校验直接报 E3604 并指明应改用 `dashscope-realtime` 提供源（避免发出必然 404 的请求）

## [0.5.3] - 2026-09-09

### Added

- 翻译插件新增 **WebSocket 实时流式 STT**（`dashscope-realtime` 提供源，DashScope run-task 任务协议）：
  - 协议实现：`wss://…/api-ws/v1/inference` + Bearer 鉴权，run-task（PCM 16k 单声道）→ 100ms 二进制帧推流 → finish-task 优雅收尾（3s 超时强制关）
  - 中间结果（sentence_end=false）经新 topic `zannen.translator/partial` 滚动推送，悬浮窗与主视图 STT 区以斜体半透明实时显示，断句（sentence_end=true）转 utterance → 翻译，不进历史
  - 默认模型 `qwen-audio-3.0-asr-flash-streaming`（即此前 404 的模型——它属 WebSocket 通道，不走 OpenAI 兼容 HTTP）；会话线程代际防护/错误分级与批量路径一致
  - 新增 tungstenite 依赖（rustls + webpki-roots，同步客户端契合 worker 线程模型）
- 后端 realtime 协议模块单测（run-task/finish-task 构造、事件解析、PCM 转换、task id 唯一性）

## [0.5.2] - 2026-09-09

### Added

- 翻译插件新增**命名配置方案**：当前 STT/翻译配置可命名保存（localStorage `zannen.translator.profiles`），下拉/胶囊一键应用切换，支持同名覆盖与删除；会话进行中锁定防误改

### Fixed

- **打开悬浮窗报错**：窗口 label 含插件 id 的 `.`（`overlay-zannen.translator`）违反 Tauri label 字符集——新增 `overlay_label()` 统一将非法字符替换为 `-`，open/close 与销毁判定共用同一映射

## [0.5.1] - 2026-09-09

### Added

- 翻译插件接入**千问 DashScope 平台**（OpenAI 兼容模式）：
  - STT 新增 `dashscope` 提供源：qwen3-asr-flash，input_audio base64 数据URI，asr_options 控制 ITN 与指定输入语言（auto 自动识别）
  - 翻译新增 `dashscope` 提供源：qwen-mt 系列（默认 qwen-mt-turbo），translation_options 源语言 auto 自动识别，目标语言 ISO 码映射 DashScope 英文全称（15 种，未知目标报 E3603）
  - 前端提供源下拉新增千问选项，切换时自动预填 DashScope 端点与模型（可自由修改）；配置校验缺密钥报 E3604

## [0.5.0] - 2026-09-09

### Added

- **新插件 Zannen 实时翻译（zannen.translator）**：麦克风输入 → 语音转文字（自动识别或指定输入语言）→ 翻译为目标语言
  - 音频管线：cpal 采集（16kHz 单声道，设备不匹配时线性重采样）→ 能量 VAD 分段（0.8s 静音判停/10s 上限）→ STT → 翻译，事件流 state/utterance/translation/error（E3601–E3604 错误码）
  - STT 提供源可自由配置：mock（离线演示）/ OpenAI 兼容（`/audio/transcriptions`，端点可改接 Groq/本地 whisper.cpp server，verbose_json 取识别语言）；翻译提供源：mock / OpenAI 兼容 chat / DeepL
  - 主窗口视图：双提供源配置卡片（端点/密钥/模型/语言）、会话控制、历史列表（最近 50 条）、麦克风权限前置告知与 E3601 被拒横幅（复用权限熔断模式）
- **壳 overlay 悬浮窗机制**：命令 `overlay_open/close` 创建透明无边框置顶悬浮窗（560×300，横向矩形）；插件经 SDK `definePlugin` 可选导出 `overlay` 组件；翻译悬浮窗为玻璃拟态三段式（拖动工具栏 + 状态点/置顶切换/关闭 → STT 展示区 → 译文展示区）
- **后台常驻**：overlay 存活时主窗口关闭改为隐藏（macOS 点 Dock 图标经 Reopen 唤回），悬浮窗独立继续工作
- 麦克风权限：Info.plist 用途声明；`open_permission_settings` 扩展 `microphone`（macOS 隐私-麦克风页 / Windows 麦克风隐私设置）

### Changed

- 构建配置：启用 `macOSPrivateApi`（macOS 透明窗口必需）；vendor 白名单新增 `@tauri-apps/api/window`

## [0.4.21] - 2026-09-09

### Added

- **权限获取失败的通知与功能熔断**（蓝牙为首例）：
  - 新增错误码 `E3201`（BLE 权限被系统拒绝/未授权），宿主 BLE 服务精确识别 btleplug `PermissionDenied` 并打码
  - 插件 `device.scan` 结果新增 `ble_error` 透传字段
  - 设备页权限通知横幅：说明权限类型与用处、可能被拒原因（弹窗点了不允许/系统设置中关闭/蓝牙未开启）、原始报错码；提供「重试权限检测」与「打开系统设置」（新增壳命令 `open_permission_settings`，macOS 直达隐私与安全性-蓝牙页 / Windows 直达蓝牙设置）
  - 被拒期间 BLE 功能自动中止（扫描跳过 BLE），串口功能不受影响；重试成功即自动恢复

## [0.4.20] - 2026-09-09

### Fixed

- **跟随系统外观模式失效**：手动选过深/浅后，`set_window_theme` 把窗口外观钉死，webview 的 `prefers-color-scheme` 被污染为窗口外观——切回 auto 时 matchMedia 读到的是旧手动选择而非真实系统主题。修复：
  - 新增原生 `system_theme` 命令读取真实系统外观（macOS 主线程 `NSApp.effectiveAppearance`；Windows 读个性化注册表 `AppsUseLightTheme`，新增 winreg 依赖）
  - auto 模式下 `set_window_theme` 改为解除钉死（`set_theme(None)` 跟随系统），前端 auto 解析改走原生命令（matchMedia 仅作回退）
  - 异步解析期间用户改模式则放弃应用；系统主题变化监听同样改走原生解析

## [0.4.19] - 2026-09-09

### Changed

- **背景光晕观感重调**：由两团边界可辨的光斑改为多团大尺寸低透明环境光（暗色三团：靛蓝/紫/微青，亮色对应提亮降饱和），整层 36px 模糊 + 边缘过扫描——无明确形状与边界，仅余柔和明暗氛围；底色渐变暗色更深邃聚拢、亮色更轻盈通透；暗/亮交叉淡化过渡保留
- **视图切换过渡优化**：插件/设置/主菜单间切换由弹簧急停改为品牌缓出曲线（0.26s）的 fade + 位移 + 微缩放 + 模糊收放；插件路由标签栏出现/收起改为高度展开 + 淡入淡出（0.24s），不再生硬闪现

## [0.4.18] - 2026-09-09

### Fixed

- **背景彩色光晕找回**：0.4.17 给 body 底色加过渡后改变了 WebKit 合成层顺序，不透明底色把负 z-index 光晕层整体遮盖——光晕层改为位于底色之上、应用内容之下（`z-index: 0` + `.app-frame` 显式 `z-index: 1` + `pointer-events: none`），暗/亮双层交叉淡化过渡保持不变

## [0.4.17] - 2026-09-09

### Fixed

- **主题切换的背景渐变过渡找回**：CSS 渐变不可过渡导致的生硬跳变，改为暗/亮双层光晕常驻 + opacity 交叉淡入淡出（0.45s 品牌缓出曲线），html/body 底色与文字色同步 0.45s 过渡；reduced-motion 下仍瞬时切换
- 外观模式默认跟随系统（auto）复核确认：无历史选择时 `theme.ts` 与首屏防闪烁脚本均默认 auto 并实时跟随系统（手动选择持久化为既有设计）

## [0.4.16] - 2026-09-09

### Added

- 升级欢迎页更新日志区新增滚动条（max-height + 细滚动条，标题行固定不随列表滚动）
- 升级/首启弹窗确认键快捷键：回车触发主按钮（焦点在可交互元素上时保留原生行为，输入法组词期间不触发）

## [0.4.15] - 2026-09-09

### Added

- **调试插件与真实固件对齐**（依据 SlimeVR-Tracker-nRF / SlimeVR-Tracker-nRF-Receiver / Adafruit_nRF52_Bootloader 仓库事实）：
  - 指纹表全面重写：VID `0x1209` 真实 VID/PID（含 bootloader 态）、USB product 字符串指纹（`usb_product_patterns`，最长匹配优先，解决应用态 PID 在型号间复用问题）、Zephyr board target 名纳入 `hw_names`、移除虚构的 NUS 服务 UUID、新增 ZannenDongle33 型号；修正 Smol=nRF52840、SmolAir=nRF52833（原误记 nRF52832/nRF54L15）、UF2 family 修正为 `0xada52840`/`0x621e937a`
  - 串口双协议探测：同时发送 JSON `identify`（调试协议）与文本 `info`（真实固件控制台），识别 `Board:` 行
  - 文本控制台解析：`battery` 应答/日志的 `Battery: NN%` 行接入状态规则引擎
  - `uf2.enter_bootloader` 发送真实固件的文本 `dfu` 命令（GPREGRET 复位路径），JSON 命令保留兼容
  - mock 传输层同步模拟真实控制台子集（`info`/`battery`/文本 `dfu`，board/SOC 与真实固件一致）
- 更新徽标点改为脉冲动画（`prefers-reduced-motion` 时静态）
- 窗口布局尺寸常量化（`shell/layout.ts` 单一来源）

### Changed

- 设置页语言选项复用 SDK `LOCALES`（消除内联重复）
- 壳版本兜底值改为读取 package.json（替代硬编码 0.3.0）
- 文档全面更新：README/ARCHITECTURE 版本与架构图（SideNav 已移除）、MODULE-DEBUGGER 协议章节（双协议 + 真实控制台 + HID 数据面 roadmap 标注）、PACKAGING-UPDATES 更新中心入口描述

### Fixed

- `cargo fmt --check` 工作区红线（6 文件）
- `gen-release-notes.mjs` 对非标准分类头（如 `### 测试`）的条目错误归入上一分类；CHANGELOG 4 处非标准头规范化，同版本同名分类合并
- 代码字符串非 ASCII 字符清理（`…`/`—` → ASCII）
- 图标资产生成链修复：2048² 源图入库（由母版导出），`build:assets` 重跑产物与入库图标逐字节一致；DMG 卷图标脚本接入 CI release 流程
- 多处陈旧注释（工具栈方位、欢迎页占位描述等）

## [0.4.14] - 2026-09-09

### Changed

- 调试插件更名为「Zannen SlimeVR调试」（英文 Zannen SlimeVR Debug）：i18n 字典、两份插件清单、蓝牙授权说明文案同步更新
- 调试设备选择下拉框迁移至壳顶栏右端插槽（`.topbar-slot`），与插件名面包屑同一行水平；原路由标签栏插槽移除

### Fixed

- 设备选择下拉菜单改为独立传送门（document.body + fixed 定位，随窗口滚动/缩放重定位）：不再受所在栏位 overflow/stacking context 裁剪，菜单展开不再撑动或裁切栏位

## [0.4.13] - 2026-09-09

### Added

- SDK 新增 `registerPluginTitles` 标题提供者注册（模块顶层调用一次即可）：壳在注册时与每次语言切换时主动重估，无需插件视图挂载
- 窗口几何动画期间自动暂停全部玻璃模糊（`zt-animating` 标记 + 引用计数），动画结束后恢复

### Fixed

- **插件文字与壳语言错乱**：旧机制（`TitlesBridge` 随视图挂载注册）在退回主菜单后切语言时注册表停留旧语言、未进入过插件时启动器回退英文——改为模块装载即注册 + 壳随语言事件重估，主菜单卡片/标签栏/面包屑与壳语言严格一致
- **窗口切换动画掉帧**：根因是玻璃模糊在 resize 逐帧重排中反复重算，动画期间暂停 blur 后重排开销大幅下降

## [0.4.12] - 2026-09-08

### Added

- 本地化标题桥扩展 `description` 字段：启动器卡片上的插件描述随语言切换（调试插件注册中英双语描述）

### Fixed

- 窗口扩展动画起始帧泛白（第三轮）：`html` 显式底色补齐、underPage 助手落点日志可见、**动画触发前按当前主题重申 NSWindow 与 WKWebView 底色**（`animate_resize` 接受 theme 参数，前端传入解析后主题）——三层兜底（窗口 / webview 底层 / 文档根）任一环节不再透白
- 系统工程审查修正：清理 SideNav 残留 CSS 与未使用的 `navHome` i18n 键、修正过期注释引用

## [0.4.11] - 2026-09-08

### Added

- 设置页新增**开机自启动**选项（默认关闭）：macOS 登录项与 Windows 注册表 Run 键双端适配，两端均无需系统授权弹窗
- 壳新增**插件本地化标题桥**（`setPluginTitles`）：插件可按当前语言注册名称与路由标题，壳标签栏/启动器/面包屑随语言实时切换——代码内清单保持英文，UI 显示恢复本土化

### Changed

- CHANGELOG 规范化（Keep a Changelog + SemVer）：分类统一为 Added/Changed/Fixed/Removed；升级欢迎页改为按 `previousVersion` **跨版本聚合**真实更新条目
- 代码命名与英文字符串规范复查：变量名、插件 `name`/`description` 等字段统一英文书写规范；本土化数据（i18n 字典/数据文件）不受影响

### Fixed

- 窗口扩缩动画起始帧泛白残留：除 NSWindow 底色外，同步设置 WKWebView `underPageBackgroundColor` 随主题——webview 未重排前的新暴露区域也不再透白
- 修复英文清扫中误伤本土化显示的问题（插件名/路由标题曾固定显示英文，现经本地化桥随语言切换）

## [0.4.10] - 2026-09-08

### Fixed

- **窗口扩缩动画泛白（再次出现）**：`backgroundColor` 配置只作用于 webview 背景；macOS 新暴露区域显示的是 NSWindow 自身底色（默认白）。已改为启动时 + 主题切换时同步设置 NSWindow 背景色为令牌底色（深 #0B0D13 / 浅 #EEF1F7），根治透底
- **调试设备选择下拉框位置**：由插件内容区右上角浮放改为经 createPortal 渲染进壳插件标签栏右侧插槽——与插件名/路由标签同一行水平；插槽未就绪时自动退化为原浮放形态

### Added

- **升级欢迎页接入真实更新日志**：构建期 `scripts/gen-release-notes.mjs` 从 CHANGELOG.md 提取最新版本条目生成 `release-notes.json`，升级弹窗展示当版真实更新内容（不再占位文案）

### Changed

- 代码字符串全英文（注释与 i18n 字典/数据文件豁免）：Rust 错误消息、日志、测试文案、插件清单标题、前端日志与测试描述全部英文化；`docs/ERROR-CODES.md` 内文案不受影响（错误码不变）

## [0.4.9] - 2026-09-08

### Changed

- 窗口扩缩动画降速调柔：放大 0.48s / 缩小 0.40s，过冲控制点由 1.36 收为 1.18（节奏更从容）

### Added

- 调试插件视图右上角浮放**调试设备选择下拉框**（插件专属）：列出已连接会话，点击切换活跃数据源，可断开；替代原壳顶栏的全局下拉

## [0.4.8] - 2026-09-08

### Fixed

- **窗口扩缩动画低帧率感**：上一版的离散步进（19 帧）降级为原生连续动画——改走 `NSAnimationContext` 分组 + 自定义 `CAMediaTimingFunction` 控制点（放大带回弹、缩小强减速），曲线可定制且系统级平滑

### Changed

- 移除壳顶栏设备选择下拉框（设备选择是 zannen.debugger / SlimeVR 调试插件的专属功能，由插件视图自身承载）；顶栏仅保留面包屑

## [0.4.7] - 2026-09-08

### Changed

- **窗口扩缩缓动非线性感**：macOS 改为进程内定时步进 + 主线程分发（保持原生顺滑的同时可定制曲线）——放大带回弹（easeOutBack 轻过冲）、缩小强减速（easeOutQuint）；Windows/其他平台插值同步该曲线
- **Windows 端视觉**：窗口启用 Mica 系统背景效果（跟随系统深浅色）+ 透明窗体，玻璃拟态与系统材质融合
- 内容视图切换过渡由线性时长改为 spring 物理曲线

## [0.4.6] - 2026-09-08

### Fixed

- **窗口扩缩动画泛白托尾与卡顿**：macOS 改走原生 `NSWindow.setFrame(_:display:animate:)`（系统级动画，无逐帧 IPC、webview 不做中间态重排）；窗口背景色固定为深色令牌底色（消除重绘前的白色透出）；非 macOS 平台退化为 12 步低频插值（替代逐帧 IPC）

## [0.4.5] - 2026-09-08

### Changed

- **窗口扩缩动画**：主菜单 ↔ 插件/设置的窗口尺寸与位置变化改为 320ms easeOutCubic 逐帧插值（中心点保持），替代瞬跳；`prefers-reduced-motion` 时直接到位
- **视图切换过渡统一**：主页 / 设置 / 插件路由全部纳入同一 AnimatePresence，fade + 轻位移 + 微缩放（220ms），与窗口动画同节奏

## [0.4.4] - 2026-09-08

### Changed

- 插件/设置子级窗口尺寸收窄为 1040×720
- 窗口扩缩改为**保持中心点不动**（读取当前窗口中心 → 变尺寸 → 反推位置），壳窗口被移动过之后进入插件不再跳回屏幕中心

## [0.4.3] - 2026-09-07

### Fixed

- **进入插件/「设备」页卡顿数秒**：壳的全部 Tauri 命令为同步函数时在主线程执行——`plugin_invoke` 的设备扫描（串口探测 + 3s BLE 扫描）把 UI 整体冻结。所有壳命令改为 async，重负载（插件调用、插件安装）经 `spawn_blocking` 离线程执行，UI 全程可响应

### Changed

- 悬浮工具栈移至**左下角**（含更新提示点位置同步）

## [0.4.2] - 2026-09-07

### Changed

- 主菜单窗口缩小为 420×840（仍竖屏），最小 400×560
- 壳工具栏（外观/语言/设置）从顶栏迁至**右下角悬浮工具栈**，全层级（主菜单/插件/设置）统一常驻；设置入口只保留悬浮栈一处（移除启动器网格中的系统卡片）；顶栏仅保留面包屑与设备选择器；有可用更新时设置按钮带提示点

## [0.4.1] - 2026-09-07

### Changed

- 主菜单（模块选择）窗口改为竖向窄屏比例（460×1150，≈512:1280）；进入插件/设置子级界面时窗口自动扩为 1180×800 工作区并居中，返回主页时收回竖屏；仅在层级翻转时调整，不打断用户在子级内的自定义尺寸

## [0.4.0] - 2026-09-07

### Changed

- **收回多窗口，改为单窗口层级导航**：启动即模块选择主页（必选模块进入）；插件界面顶部标签栏带「← 模块」返回上级；壳内建视图不显示标签栏
- **设置入口全层级常驻**：顶栏右侧齿轮（任意界面可达）；设置视图含外观（主题三段）与语言快捷设置 + 更新中心；从设置返回还原到进入前位置
- 插件子级导航区合并壳设置入口（顶栏齿轮在所有层级可用）
- 移除多窗口机制（`open_plugin_window` 命令、`plugin-*` 窗口、窗口角色解析），导航状态机简化为 `selection` + `settingsFrom`

## [0.3.0] - 2026-09-07

### Added

- **模块选择主页**（类 nRF Connect 启动器）：插件卡片网格（图标/描述/版本/路由胶囊直达），启动默认落在主页；侧边栏品牌区与「模块」项可随时返回
- **启动器与插件窗口分离**：点击模块卡片打开独立插件窗口（`plugin-<id>`，已打开则聚焦并切换子路由）；插件窗口带路由标签栏；多窗口主题/语言经 localStorage storage 事件实时同步
- **蓝牙授权前置告知**（macOS）：系统授权框前先弹应用内说明（为什么需要 BLE）；「继续并授权」才触发系统弹窗；「暂不」时只做串口扫描（新增 `device.scan` 的 `skip_ble` 参数）

### Changed

- **发布版移除 mock 虚拟设备**：`mock-transport` 改为壳 feature 门控，仅 `pnpm dev`（`tauri dev --features mock`）启用；正式安装包的设备列表只呈现真实硬件
- 修复 `src-tauri` Cargo.toml 中 serde/tokio/log/env_logger 误落 macOS-only 段（会导致 Windows 构建缺依赖）

### Changed

- 启动链路并行化：总线桥接 / 壳信息与首启识别 / 插件清单三路并发，首帧渲染不等待
- 插件前端：清单注册即注入 CSS，空闲预装载全部插件模块（上次已合入），首击零等待

## [0.2.1] - 2026-09-06

### Fixed

- **插件前端加载失败**：CSP `script-src` 缺 `blob:`（index.html meta 与 tauri.conf.json 不一致，交集取严导致 blob import 被拦）；dev 配置 `connect-src` 同步补 `plugin-asset:`。新增 CSP 防回归测试。
- **插件模块 import 报 "does not resolve to a valid URL"**：WKWebView 对 blob: 模块内的 host 相对路径解析失败，vendor specifier 重写改为带 origin 的完整 URL。
- **插件模块报 "Importing binding name 'X' is not found"**：vite/rollup 默认 `minifyInternalExports` 把 vendor 入口的具名导出压成单字母且 tree-shake 掉壳未用的导出；已设 `preserveEntrySignatures: "strict"` + `minifyInternalExports: false`。

### Added

- 全系统错误识别码（`[Exxxx]` 前缀）：壳前端 / 插件 ABI / 硬件服务 / 更新安装四段号段，目录见 docs/ERROR-CODES.md。

### Fixed

- **macOS 启动闪退**：`NSBluetoothAlwaysUsageDescription` 缺失——首页设备自动扫描触发 CoreBluetooth 初始化，TCC 因无用途描述直接杀进程（SIGABRT）。新增 `src-tauri/Info.plist` 声明蓝牙用途（Tauri 自动合并），首次扫描时系统正常弹出授权。

### Changed

- 插件前端装载提速：清单注册后立即注入插件 CSS（防样式闪烁），空闲时（requestIdleCallback，老 WebView 退化为短延时）按"当前选中优先"后台预装载全部插件模块——首次点击零等待，同时保留懒装载的启动提速。

## [0.2.0] - 2026-09-06

从脚手架到功能完善版本。

### Added

- **BLE 数据面**：NUS 连接/写入/通知订阅（`ble.connect/write/close`），BLE 会话与串口会话统一；mock 虚拟 BLE 设备可无硬件演示
- **MCUboot 串行 DFU（nRF54）**：完整 SMP over serial 协议栈（SLIP 分片、CBOR、CRC16、偏移校验状态机），支持上传/确认/复位；固件视图按设备能力自动分流 UF2 / MCUboot；内置内存 MCUboot 模拟器可无硬件演示
- **串口热插拔**：1s 轮询 diff，`serial.plugged/unplugged` 事件；拔出自动清理会话并移除设备
- **插件前端懒装载**：启动仅加载清单，首次选中才装载插件模块；失败可重试
- **i18n**：壳与调试插件全量中英文，跟随系统 + 顶栏手动切换
- **插件包 ed25519 签名**：`scripts/sign-plugin.mjs` 打包签名，安装管线验签（篡改/未签名拒绝，开发可显式放行）
- **主题系统**：auto/light/dark 三模式，跟随系统 + 顶栏手动切换；Windows 任务栏与 macOS Dock 图标随主题切换（暗/亮双版图标）；安装包图标为 📦 纸箱变体
- **品牌化安装包**：NSIS 定制视觉 + DMG 品牌化布局；全新安装/升级识别（注册表 + 首启状态双通道）与首启动效页
- **在线更新**：本体（tauri-plugin-updater，签名更新包）+ 插件（自研管线：manifest 检查 → 校验 → 原子替换 → 热重载）；更新中心视图
- **终端增强**：命令历史（↑/↓）、时间戳开关、日志导出
- **时间轴增强**：图例点击开关通道、CSV 导出、暂停游标读数
- **多设备**：多会话并存，时间轴/3D 视图按设备分轨选择
- **CI**：ci.yml（fmt/clippy/test/前端/交叉检查）+ release.yml（macOS universal2 + Windows x64 NSIS 签名产物 + Release 草稿）

### Fixed

- BLE 指纹匹配改为最长模式优先（修复 SmolAir 误配 Smol）
- 侧边栏版本号动态读取（`shell_info`）
- 插件加载失败有错误占位而非静默

### Changed

- 3D 视图四元数读数降频更新（30Hz→4Hz），消除高频 setState

### Added

- Rust 35 项单元测试 + mock 端到端集成测试（扫描→连接→数据流→命令→断开）
- 前端 vitest 42 例（rewriteImports/compareSemver/hex/缓冲分桶/CSV 等）
- mock MCUboot 全链路 DFU 测试（上传→确认→复位）

## [0.1.0] - 2026-09-04

首个可运行脚手架：壳 + 插件系统（原生动态库 ABI）+ 调试模块六视图 + mock 虚拟设备。
