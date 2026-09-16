/**
 * 插件模块契约。
 *
 * 插件前端入口（manifest.frontend.entry）的默认导出必须是 PluginModule：
 *
 * ```tsx
 * export default definePlugin({
 *   routes: {
 *     devices: DevicesView,   // 与 plugin.toml [[routes]] 的 path 对应
 *     console: ConsoleView,
 *   },
 * });
 * ```
 */

import type { ComponentType } from "react";

export interface PluginModule {
  /** path → 视图组件。path 与清单 routes[].path 对应。 */
  routes: Record<string, ComponentType>;
  /**
   * 可选：悬浮窗 overlay 组件。壳命令 `overlay_open(plugin)` 创建的
   * `?overlay=<plugin>` 窗口（透明、无边框、置顶）中由 OverlayHost 渲染；
   * 工具栏/关闭等窗口 chrome 由组件自绘（data-tauri-drag-region 拖动）。
   */
  overlay?: ComponentType;
  /** 可选：模块挂载/卸载钩子。 */
  onMount?: () => void;
  onUnmount?: () => void;
}

/** 类型辅助：定义插件模块。 */
export function definePlugin(module: PluginModule): PluginModule {
  return module;
}

/** 插件清单类型（与 Rust `PluginManifest` 对应）。 */
export interface PluginManifestDto {
  id: string;
  name: string;
  version: string;
  api: number;
  description: string;
  icon?: string | null;
  frontend?: { entry: string; css?: string | null } | null;
  backend?: { name: string } | null;
  capabilities: string[];
  routes: { path: string; title: string; icon: string }[];
}
