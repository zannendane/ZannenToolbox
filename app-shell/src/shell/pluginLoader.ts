/**
 * 插件前端装载器。
 *
 * 流程：plugin-asset:// 拉取插件 ESM 源码 → 重写裸导入 specifier 到壳的
 * vendor URL（保证 react / sdk 与壳同实例）→ Blob URL 动态 import。
 *
 * 采用"重写 + blob import"而非 import map，原因：
 * - WKWebView 的 import map 需要 Safari 16.4+（macOS 13.3+），blob import 兼容面更宽；
 * - blob URL 可携带源码映射注释，调试体验一致；
 * - 失败时可精确降级（错误占位视图），不阻塞壳启动。
 */

import type { PluginManifestDto, PluginModule } from "@zannen/plugin-sdk";
import { SHELL_ERR, codedError, errorText } from "./errors";
import { useShellStore, type LoadedUiPlugin } from "./store";

/** 裸导入 specifier → vendor 条目名。 */
const VENDOR_MAP: Record<string, string> = {
  react: "vendor/react",
  "react/jsx-runtime": "vendor/react-jsx-runtime",
  "react-dom": "vendor/react-dom",
  "react-dom/client": "vendor/react-dom",
  "@zannen/plugin-sdk": "vendor/zannen-plugin-sdk",
  "@tauri-apps/api/core": "vendor/tauri-core",
  "@tauri-apps/api/event": "vendor/tauri-events",
  "@tauri-apps/api/window": "vendor/tauri-window",
  "@tauri-apps/plugin-dialog": "vendor/tauri-plugin-dialog",
  "framer-motion": "vendor/framer-motion",
};

function vendorUrl(entry: string): string {
  // blob: 模块在 WKWebView 中无法解析 host 相对路径（"does not resolve to a valid URL"），
  // 必须给完整 URL。origin 在 dev 为 http://localhost:1420，产物为 tauri://localhost。
  const origin = globalThis.location?.origin ?? "";
  return import.meta.env.DEV
    ? `${origin}/src/${entry}.ts`
    : `${origin}/assets/${entry}.js`;
}

/** 用给定的解析器重写 ESM 源码中的导入 specifier（纯函数，供测试）。 */
export function rewriteImportsWith(
  source: string,
  resolveSpecifier: (specifier: string) => string | null,
): string {
  const pattern = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)["']([^"']+)["']/g;
  return source.replace(pattern, (whole, prefix: string, specifier: string) => {
    const url = resolveSpecifier(specifier);
    return url ? `${prefix}"${url}"` : whole;
  });
}

/** 重写插件 ESM 中的裸导入 specifier 为绝对 vendor URL。 */
export function rewriteImports(source: string): string {
  return rewriteImportsWith(source, (specifier) => {
    const vendor = VENDOR_MAP[specifier];
    return vendor ? vendorUrl(vendor) : null;
  });
}

function assetBase(pluginId: string): string {
  // Windows WebView2 的自定义协议映射为 http://<scheme>.localhost
  const isWindows = navigator.userAgent.includes("Windows");
  const origin = isWindows ? "http://plugin-asset.localhost" : "plugin-asset://localhost";
  return `${origin}/${pluginId}`;
}

function injectCssOnce(id: string, url: string): void {
  const tagId = `plugin-css-${id}`;
  if (document.getElementById(tagId)) return;
  const link = document.createElement("link");
  link.id = tagId;
  link.rel = "stylesheet";
  link.href = url;
  document.head.appendChild(link);
}

/** 预热插件静态资产（当前为 CSS；模块本体仍由 ensurePluginLoaded 装载）。 */
export function preloadPluginAssets(manifest: PluginManifestDto): void {
  if (manifest.frontend?.css) {
    injectCssOnce(manifest.id, `${assetBase(manifest.id)}/${manifest.frontend.css}`);
  }
}

/** 装载单个插件前端。失败返回带 error 的记录（含错误识别码），不抛出。 */
export async function loadPluginFrontend(manifest: PluginManifestDto): Promise<LoadedUiPlugin> {
  if (!manifest.frontend) {
    return { manifest };
  }
  try {
    const base = assetBase(manifest.id);
    const entryUrl = `${base}/${manifest.frontend.entry}`;
    let res: Response;
    try {
      res = await fetch(entryUrl);
    } catch (e) {
      throw codedError(SHELL_ERR.PLUGIN_FETCH_FAILED, `failed to fetch plugin entry: ${errorText(e)}`);
    }
    if (!res.ok) {
      throw codedError(SHELL_ERR.PLUGIN_FETCH_FAILED, `failed to fetch plugin entry (HTTP ${res.status})`);
    }
    const source = rewriteImports(await res.text());
    const blobUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    let mod: { default?: PluginModule };
    try {
      mod = await import(/* @vite-ignore */ blobUrl);
    } catch (e) {
      throw codedError(SHELL_ERR.PLUGIN_IMPORT_FAILED, `plugin module import failed: ${errorText(e)}`);
    } finally {
      URL.revokeObjectURL(blobUrl);
    }
    if (!mod.default || typeof mod.default !== "object" || !mod.default.routes) {
      throw codedError(SHELL_ERR.PLUGIN_NO_MODULE, "plugin entry lacks default PluginModule export");
    }
    if (manifest.frontend.css) {
      injectCssOnce(manifest.id, `${base}/${manifest.frontend.css}`);
    }
    return { manifest, module: mod.default };
  } catch (e) {
    return { manifest, error: errorText(e) };
  }
}

/** 进行中的装载 Promise（防重入）。 */
const pendingLoads = new Map<string, Promise<LoadedUiPlugin>>();

/**
 * 按需装载插件前端（懒装载入口，幂等）。
 *
 * - 已装载 / 无前端 → 直接返回现状；
 * - 进行中 → 复用同一 Promise；
 * - 完成后写回 store（失败记录 error，由 UI 提供重试）。
 */
export function ensurePluginLoaded(pluginId: string): Promise<LoadedUiPlugin | null> {
  const plugin = useShellStore.getState().plugins.find((p) => p.manifest.id === pluginId);
  if (!plugin) return Promise.resolve(null);
  if (plugin.module || !plugin.manifest.frontend) return Promise.resolve(plugin);
  const pending = pendingLoads.get(pluginId);
  if (pending) return pending;
  const task = loadPluginFrontend(plugin.manifest).then((loaded) => {
    pendingLoads.delete(pluginId);
    if (loaded.error) {
      console.error(`[shell] plugin frontend load failed ${pluginId}:`, loaded.error);
    }
    useShellStore.getState().replacePlugin(loaded);
    return loaded;
  });
  pendingLoads.set(pluginId, task);
  return task;
}
