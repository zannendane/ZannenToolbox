/**
 * 壳状态：插件装载、设备列表、导航选择（zustand）。
 */

import { create } from "zustand";
import type { PluginLocalizedTitles, PluginManifestDto, PluginModule } from "@zannen/plugin-sdk";

export interface LoadedUiPlugin {
  manifest: PluginManifestDto;
  module?: PluginModule;
  error?: string;
}

export interface DeviceInfoDto {
  id: string;
  kind?: string | null;
  label: string;
  transports: string[];
  capabilities: string[];
  extra?: Record<string, unknown>;
}

export interface NavSelection {
  pluginId: string;
  route: string;
}

interface ShellState {
  plugins: LoadedUiPlugin[];
  pluginsReady: boolean;
  selection: NavSelection | null;
  devices: Record<string, DeviceInfoDto>;
  selectedDeviceId: string | null;
  /** 本体有可用更新时的版本号（设置入口徽标点）。 */
  appUpdateAvailable: string | null;
  /** 进入设置前的位置（返回时还原）。 */
  settingsFrom: NavSelection | null;
  /** 插件注册的本地化标题（displayName/routes 覆盖 manifest 默认英文）。 */
  pluginTitles: Record<string, PluginLocalizedTitles>;

  setPlugins: (plugins: LoadedUiPlugin[]) => void;
  replacePlugin: (plugin: LoadedUiPlugin) => void;
  select: (sel: NavSelection) => void;
  /** 回到模块选择主页。 */
  goHome: () => void;
  /** 打开壳设置（记住当前位置）。 */
  openSettings: () => void;
  /** 关闭设置，回到进入前的位置。 */
  closeSettings: () => void;
  upsertDevice: (d: DeviceInfoDto) => void;
  removeDevice: (id: string) => void;
  selectDevice: (id: string | null) => void;
  setAppUpdateAvailable: (version: string | null) => void;
  setPluginTitles: (pluginId: string, titles: PluginLocalizedTitles) => void;
  /** 合并批量标题（语言切换时由提供者重估结果写入）。 */
  mergePluginTitles: (titles: Record<string, PluginLocalizedTitles>) => void;
  /** 插件路由本地化标题（回退 manifest.title）。 */
  routeTitle: (pluginId: string, route: string, fallback: string) => string;
  /** 插件本地化名称（回退 manifest.name）。 */
  pluginName: (pluginId: string, fallback: string) => string;
  /** 插件本地化描述（回退 manifest.description）。 */
  pluginDesc: (pluginId: string, fallback: string) => string;
}

export const useShellStore = create<ShellState>((set, get) => ({
  plugins: [],
  pluginsReady: false,
  selection: null,
  devices: {},
  selectedDeviceId: null,
  appUpdateAvailable: null,
  settingsFrom: null,
  pluginTitles: {},

  setPlugins: (plugins) =>
    // 默认落在模块选择主页（selection 保持 null），不再自动进入首个插件
    set({ plugins, pluginsReady: true }),
  replacePlugin: (plugin) =>
    set((state) => ({
      plugins: state.plugins.map((p) => (p.manifest.id === plugin.manifest.id ? plugin : p)),
    })),
  select: (selection) => set({ selection }),
  goHome: () => set({ selection: null, settingsFrom: null }),
  openSettings: () =>
    set((state) => ({
      settingsFrom: state.selection,
      selection: { pluginId: "shell", route: "settings" },
    })),
  closeSettings: () =>
    set((state) => ({
      selection: state.settingsFrom,
      settingsFrom: null,
    })),
  upsertDevice: (d) =>
    set((state) => ({ devices: { ...state.devices, [d.id]: d } })),
  removeDevice: (id) =>
    set((state) => {
      const devices = { ...state.devices };
      delete devices[id];
      return {
        devices,
        selectedDeviceId: state.selectedDeviceId === id ? null : state.selectedDeviceId,
      };
    }),
  selectDevice: (selectedDeviceId) => set({ selectedDeviceId }),
  setAppUpdateAvailable: (appUpdateAvailable) => set({ appUpdateAvailable }),
  setPluginTitles: (pluginId, titles) =>
    set((state) => ({ pluginTitles: { ...state.pluginTitles, [pluginId]: titles } })),
  mergePluginTitles: (titles) =>
    set((state) => ({ pluginTitles: { ...state.pluginTitles, ...titles } })),
  routeTitle: (pluginId, route, fallback) =>
    get().pluginTitles[pluginId]?.routes?.[route] ?? fallback,
  pluginName: (pluginId, fallback) =>
    get().pluginTitles[pluginId]?.name ?? fallback,
  pluginDesc: (pluginId, fallback) =>
    get().pluginTitles[pluginId]?.description ?? fallback,
}));

/** 标题提供者注册表（模块级，随插件模块装载填充；壳在语言切换时重估）。 */
const titleProviders = new Map<string, () => PluginLocalizedTitles>();

/** 重估全部提供者并合并写入 store（单个提供者异常不影响其他插件）。 */
function refreshTitlesFromProviders(): void {
  if (titleProviders.size === 0) return;
  const next: Record<string, PluginLocalizedTitles> = {};
  for (const [pluginId, provider] of titleProviders) {
    try {
      next[pluginId] = provider();
    } catch {
      // Provider failure must not break title refresh for other plugins.
    }
  }
  useShellStore.getState().mergePluginTitles(next);
}

/** 挂接 SDK 标题桥（在总线初始化时调用一次）。 */
export function initTitleBridge(): void {
  // 兼容旧式一次性注册（setPluginTitles）。
  window.addEventListener("zannen-plugin-titles", (e) => {
    const { pluginId, titles } = (e as CustomEvent).detail;
    useShellStore.getState().setPluginTitles(pluginId, titles);
  });
  // 提供者注册：立即求值一次，之后随语言切换由壳主动重估。
  window.addEventListener("zannen-plugin-titles-provider", (e) => {
    const { pluginId, provider } = (e as CustomEvent).detail as {
      pluginId: string;
      provider: () => PluginLocalizedTitles;
    };
    titleProviders.set(pluginId, provider);
    refreshTitlesFromProviders();
  });
  // 语言切换时重估全部提供者：插件视图未挂载时主菜单/标签栏也能同步。
  window.addEventListener("zannen-locale", refreshTitlesFromProviders);
}
