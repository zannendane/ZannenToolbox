/**
 * 主题系统：跟随系统（auto）/ 浅色 / 深色。
 *
 * - 解析结果写入 `document.documentElement.dataset.theme`（"dark" | "light"），
 *   全部 zt-* 令牌经 CSS 变量响应——壳与所有插件天然同步（同一文档）。
 * - `auto` 模式下监听系统 `prefers-color-scheme` 变化实时跟随。
 * - 手动选择持久化到 localStorage（启动时由 index.html 内联脚本先行应用，避免闪烁）。
 * - 每次变化同步原生侧：窗口主题（titlebar/对话框）与应用图标（深浅色双版）。
 * - 插件经 SDK 的 `useTheme()` 订阅（自定义事件 `zannen-theme`）。
 */

import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

export type ThemeMode = "auto" | "dark" | "light";
export type ResolvedTheme = "dark" | "light";

const STORAGE_KEY = "zannen-theme";

interface ThemeState {
  mode: ThemeMode;
  resolved: ResolvedTheme;
  setMode: (mode: ThemeMode) => void;
}

function systemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function resolve(mode: ThemeMode): ResolvedTheme {
  return mode === "auto" ? systemTheme() : mode;
}

/**
 * 解析 auto 模式的真实系统外观：优先原生侧（`system_theme` 命令）。
 * 手动钉过窗口主题后，webview 的 prefers-color-scheme 会被污染为窗口外观，
 * matchMedia 不可信；原生侧读取系统级外观（NSApp.effectiveAppearance /
 * Windows 个性化注册表）不受影响。失败时回退 matchMedia。
 */
async function resolveAutoNative(): Promise<ResolvedTheme> {
  try {
    const t = await invoke<string>("system_theme");
    if (t === "dark" || t === "light") return t;
  } catch {
    // 命令不可用时回退 matchMedia
  }
  return systemTheme();
}

/** 应用解析后的主题：DOM + 原生窗口 + 图标。mode 用于原生侧决定钉死/解除钉死。 */
function applyTheme(resolved: ResolvedTheme, mode: ThemeMode) {
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
  window.dispatchEvent(new CustomEvent<ResolvedTheme>("zannen-theme", { detail: resolved }));
  // 原生侧同步（失败不影响 UI）
  void invoke("set_window_theme", { theme: resolved, mode }).catch(() => {});
  void invoke("set_app_icon", { theme: resolved }).catch(() => {});
}

function readInitialMode(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "dark" || v === "light" || v === "auto" ? v : "auto";
}

/** auto 模式：原生解析真实系统外观后应用（期间用户改了模式则放弃）。 */
function applyAutoTheme() {
  void resolveAutoNative().then((resolved) => {
    if (useThemeStore.getState().mode !== "auto") return;
    useThemeStore.setState({ resolved });
    applyTheme(resolved, "auto");
  });
}

export const useThemeStore = create<ThemeState>((set) => ({
  mode: readInitialMode(),
  resolved: resolve(readInitialMode()),
  setMode: (mode) => {
    localStorage.setItem(STORAGE_KEY, mode);
    if (mode === "auto") {
      set({ mode });
      applyAutoTheme();
      return;
    }
    set({ mode, resolved: mode });
    applyTheme(mode, mode);
  },
}));

/** 启动初始化：应用当前主题并挂上系统主题监听（auto 时跟随）。 */
export function initTheme(): void {
  const { mode } = useThemeStore.getState();
  if (mode === "auto") {
    // 首帧已由 theme-init.js 落位（启动时窗口未钉主题，matchMedia 可信），
    // 再经原生侧校正一次（覆盖曾被钉主题的场景）。
    applyAutoTheme();
  } else {
    applyTheme(mode, mode);
  }
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (useThemeStore.getState().mode === "auto") applyAutoTheme();
  });
  // 多窗口同步：其他窗口写入 localStorage 时跟随
  window.addEventListener("storage", (e) => {
    if (e.key !== STORAGE_KEY || !e.newValue) return;
    const mode = e.newValue as ThemeMode;
    if ((mode === "auto" || mode === "dark" || mode === "light") && mode !== useThemeStore.getState().mode) {
      useThemeStore.getState().setMode(mode);
    }
  });
}
