/**
 * 壳界面文案（zh-CN / en-US）。
 *
 * 用 SDK 的 `createI18n` 定义两份完整字典；`useT()` 返回绑定当前语言的
 * 翻译函数，支持 `{name}` 形式的插值变量。插件清单里的路由 title 属于
 * 插件数据，不在壳字典覆盖范围内。
 */

import { createI18n, useLocale } from "@zannen/plugin-sdk";
import { useCallback } from "react";

const DICTS = {
  "zh-CN": {
    // 顶栏
    crumbSettings: "系统 · 设置",
    crumbHome: "模块",
    settingsTitle: "设置",
    settingsDesc: "壳级设置：外观、语言与更新",
    settingsBack: "返回",
    backToModules: "模块",
    appearanceSection: "外观与语言",
    autostartTitle: "开机自启动",
    autostartDesc: "登录后自动启动 ZannenToolbox。默认关闭；macOS 与 Windows 均无需系统授权弹窗。",
    launcherTitle: "选择模块",
    launcherDesc: "已装载的功能模块（类 nRF Connect 的模块启动器）",
    launcherEmpty: "未发现任何插件",
    launcherEmptyDesc: "请将插件目录放入应用 plugins/ 目录，或设置 ZANNEN_PLUGIN_DIR 后重启。",
    theme: "外观",
    themeAuto: "跟随系统",
    themeLight: "浅色",
    themeDark: "深色",
    language: "语言",
    selectDevice: "选择设备",
    noDevices: "未发现设备",
    deviceMenuEmpty: "在「设备」页执行扫描",
    // 标题栏
    minimize: "最小化",
    maximize: "最大化",
    close: "关闭",
    // 侧边导航
    loadFailed: "加载失败",
    updates: "更新",
    newVersion: "新版本",
    // 首启 / 升级欢迎页
    welcomeTitle: "欢迎使用 ZannenToolbox",
    welcomeSub: "Zannen 硬件的模块化调试工具箱",
    featureAutoDetectTitle: "硬件自动识别",
    featureAutoDetectDesc: "串口探测 + BLE 指纹，插上即识别",
    featureFlashTitle: "UF2 固件刷写",
    featureFlashDesc: "三步刷写流，family 校验防刷错",
    featureDataVizTitle: "实时数据可视化",
    featureDataVizDesc: "传感器与 RF 原始数据时间轴",
    feature3dTitle: "3D 运动渲染",
    feature3dDesc: "跟踪器姿态的实时三维呈现",
    upgradeTitle: "升级完成",
    upgradeNotesTitle: "本次更新",
    upgradeNote1: "模块化插件系统与调试模块基线",
    upgradeNote2: "品牌化安装包与自动升级识别",
    upgradeNote3: "本体 + 插件双轨在线更新",
    upgradeNotesHint: "正式更新日志将在更新源配置后自动拉取。",
    ctaExplore: "开始探索",
    ctaEnter: "进入工作台",
    // 更新中心
    updateCenterTitle: "更新中心",
    updateCenterDesc: "本体与插件的在线更新（更新源未配置时全部静默跳过）",
    updateSourcesMissing:
      "更新源未配置（占位状态）。替换应用数据目录的 update-sources.json 后启用；离线安装不受影响。",
    appCardTitle: "ZannenToolbox 本体",
    currentVersion: "当前",
    updateFailed: "更新失败",
    restartToUpdate: "重启完成更新",
    downloadAndInstall: "下载并安装",
    checkUpdates: "检查更新",
    upToDate: "已是最新",
    loadedPlugins: "已装载插件",
    latest: "最新",
    updated: "已更新",
    failed: "失败",
    check: "检查",
    noUpdateSource: "该插件未配置更新源",
    noPluginsLoaded: "未装载任何插件。",
    // 欢迎视图
    noPluginsFound: "未发现任何插件",
    noPluginsHint: "请将插件目录放入应用 plugins/ 目录，或设置 ZANNEN_PLUGIN_DIR 后重启。",
    pickModule: "从左侧选择一个模块开始",
    loadFailedNotice: "注意：{n} 个插件前端加载失败",
    modulesLoaded: "已装载 {n} 个模块",
    // 插件宿主
    loadingPlugins: "正在装载插件…",
    loadingPlugin: "正在加载「{name}」…",
    viewError: "插件视图发生错误",
    unknownBuiltinView: "未知内建视图",
    pluginNotFound: "插件不存在",
    pluginLoadFailed: "插件「{name}」前端加载失败",
    overlayNotAvailable: "该插件未提供悬浮窗",
    routeNotImplemented: "该路由未实现",
    retry: "重试",
  },
  "en-US": {
    // TopBar
    crumbSettings: "System · Settings",
    crumbHome: "Modules",
    settingsTitle: "Settings",
    settingsDesc: "Shell settings: appearance, language and updates",
    settingsBack: "Back",
    backToModules: "Modules",
    appearanceSection: "Appearance & Language",
    autostartTitle: "Launch at login",
    autostartDesc: "Start ZannenToolbox automatically after signing in. Off by default; no system permission prompt is required on macOS or Windows.",
    launcherTitle: "Choose a module",
    launcherDesc: "Installed feature modules (nRF Connect-style launcher)",
    launcherEmpty: "No plugins found",
    launcherEmptyDesc: "Place plugin directories under the app's plugins/ directory, or set ZANNEN_PLUGIN_DIR and restart.",
    theme: "Appearance",
    themeAuto: "System",
    themeLight: "Light",
    themeDark: "Dark",
    language: "Language",
    selectDevice: "Select device",
    noDevices: "No devices found",
    deviceMenuEmpty: "Run a scan on the Devices page",
    // TitleBar
    minimize: "Minimize",
    maximize: "Maximize",
    close: "Close",
    // SideNav
    loadFailed: "failed to load",
    updates: "Updates",
    newVersion: "New version",
    // Welcome flow
    welcomeTitle: "Welcome to ZannenToolbox",
    welcomeSub: "A modular debugging toolbox for Zannen hardware",
    featureAutoDetectTitle: "Automatic hardware detection",
    featureAutoDetectDesc: "Serial probing + BLE fingerprinting, plug and play",
    featureFlashTitle: "UF2 firmware flashing",
    featureFlashDesc: "Three-step flashing flow with family verification",
    featureDataVizTitle: "Real-time data visualization",
    featureDataVizDesc: "Timelines of raw sensor and RF data",
    feature3dTitle: "3D motion rendering",
    feature3dDesc: "Real-time 3D rendering of tracker orientation",
    upgradeTitle: "Upgrade complete",
    upgradeNotesTitle: "In this update",
    upgradeNote1: "Modular plugin system with a baseline debugger module",
    upgradeNote2: "Branded installers and automatic upgrade detection",
    upgradeNote3: "Dual-track online updates for the app and plugins",
    upgradeNotesHint: "Release notes will be fetched once an update source is configured.",
    ctaExplore: "Start exploring",
    ctaEnter: "Enter workbench",
    // Update center
    updateCenterTitle: "Update Center",
    updateCenterDesc: "Online updates for the app and plugins (silently skipped without sources)",
    updateSourcesMissing:
      "No update source configured (placeholder). Replace update-sources.json in the app data directory to enable; offline installs are unaffected.",
    appCardTitle: "ZannenToolbox App",
    currentVersion: "Current",
    updateFailed: "Update failed",
    restartToUpdate: "Restart to update",
    downloadAndInstall: "Download & install",
    checkUpdates: "Check for updates",
    upToDate: "Up to date",
    loadedPlugins: "Loaded plugins",
    latest: "Latest",
    updated: "Updated",
    failed: "Failed",
    check: "Check",
    noUpdateSource: "No update source configured for this plugin",
    noPluginsLoaded: "No plugins loaded.",
    // Welcome view
    noPluginsFound: "No plugins found",
    noPluginsHint:
      "Place plugin directories in the app plugins/ directory, or set ZANNEN_PLUGIN_DIR and restart.",
    pickModule: "Pick a module from the sidebar to start",
    loadFailedNotice: "Note: {n} plugin frontend(s) failed to load",
    modulesLoaded: "{n} module(s) loaded",
    // Plugin host
    loadingPlugins: "Loading plugins…",
    loadingPlugin: "Loading {name}…",
    viewError: "The plugin view ran into an error",
    unknownBuiltinView: "Unknown built-in view",
    pluginNotFound: "Plugin not found",
    pluginLoadFailed: "Failed to load the frontend of \"{name}\"",
    overlayNotAvailable: "This plugin does not provide an overlay",
    routeNotImplemented: "This route is not implemented",
    retry: "Retry",
  },
};

const translate = createI18n(DICTS);

export type ShellI18nKey = keyof (typeof DICTS)["zh-CN"];

/** 绑定当前语言的翻译函数（`useLocale()` 订阅 + `{name}` 插值）。 */
export function useT() {
  const locale = useLocale();
  return useCallback(
    (key: ShellI18nKey, vars?: Record<string, string | number>) => {
      let text = translate(locale, key);
      if (vars) {
        for (const [name, value] of Object.entries(vars)) {
          text = text.replaceAll(`{${name}}`, String(value));
        }
      }
      return text;
    },
    [locale],
  );
}
