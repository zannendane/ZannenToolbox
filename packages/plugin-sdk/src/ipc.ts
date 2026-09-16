/**
 * 插件 IPC：调用插件后端 + 订阅宿主事件总线。
 *
 * 前端链路：plugin_invoke（Tauri 命令）→ 插件 invoke；
 * 上行链路：宿主总线事件 → Tauri `zannen-bus` 事件 → 按 topic 分发。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 总线事件信封。 */
export interface BusEnvelope<T = unknown> {
  topic: string;
  payload: T;
}

/** 调用指定插件的后端方法。 */
export function invokePlugin<T = unknown>(
  plugin: string,
  method: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  return invoke<T>("plugin_invoke", { plugin, method, args });
}

/** 调用壳内建命令。 */
export function invokeShell<T = unknown>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>(command, args);
}

/** 订阅总线主题（精确匹配或谓词）。返回取消订阅函数。 */
export function onBus<T = unknown>(
  topic: string | ((topic: string) => boolean),
  cb: (payload: T, topic: string) => void,
): Promise<UnlistenFn> {
  const matches = typeof topic === "string" ? (t: string) => t === topic : topic;
  return listen<BusEnvelope<T>>("zannen-bus", (event) => {
    if (matches(event.payload.topic)) {
      cb(event.payload.payload, event.payload.topic);
    }
  });
}

/** 插件向壳注册本地化显示标题（名称与路由标题），壳展示层随语言切换。 */
export interface PluginLocalizedTitles {
  name?: string;
  description?: string;
  routes?: Record<string, string>;
}

export function setPluginTitles(pluginId: string, titles: PluginLocalizedTitles): void {
  window.dispatchEvent(
    new CustomEvent("zannen-plugin-titles", { detail: { pluginId, titles } }),
  );
}

/**
 * 标题提供者：壳在注册时与每次语言切换时调用，实现内部应自行读取当前语言
 * （`currentLocale()`），返回值结构与 PluginLocalizedTitles 相同。
 */
export type PluginTitlesProvider = () => PluginLocalizedTitles;

/**
 * 注册本地化标题提供者（推荐方式，模块顶层调用一次即可）。
 *
 * 与 setPluginTitles 的区别：壳在每次语言切换时重新调用 provider，
 * 无需插件视图处于挂载状态——主菜单卡片 / 标签栏 / 面包屑始终与壳语言一致。
 */
export function registerPluginTitles(pluginId: string, provider: PluginTitlesProvider): void {
  window.dispatchEvent(
    new CustomEvent("zannen-plugin-titles-provider", { detail: { pluginId, provider } }),
  );
}
