/**
 * 在线更新服务（本体 + 插件双轨）。
 *
 * - 本体：tauri-plugin-updater（签名更新包，endpoints 在 tauri.conf.json 的 plugins.updater）
 * - 插件：manifest 检查（tauri-plugin-http）→ 下载 .znplugin → 临时文件（plugin-fs）
 *   → Rust `plugin_install`（校验 + 原子替换 + 热重载）
 * - 更新源配置：Rust 命令 `update_sources`（app_data 覆盖 > 内置占位）
 * - 全部网络失败/未配置 → 静默返回，绝不打扰用户（离线安装场景一等公民）
 */

import { invoke } from "@tauri-apps/api/core";
import { join, tempDir } from "@tauri-apps/api/path";
import { writeFile } from "@tauri-apps/plugin-fs";
import { fetch } from "@tauri-apps/plugin-http";
import { check, type Update } from "@tauri-apps/plugin-updater";

export interface UpdateSources {
  app: { enabled: boolean; manifest: string };
  plugins: Record<string, { enabled: boolean; manifest: string }>;
}

export interface RemotePluginManifest {
  version: string;
  min_api?: number;
  notes?: string;
  files: Record<string, { url: string; sha256: string; signature?: string }>;
}

export interface PluginUpdateInfo {
  pluginId: string;
  version: string;
  notes?: string;
  fileUrl: string;
  sha256: string;
  /** 包文件的 ed25519 签名（hex，对 .znplugin 完整字节） */
  signature?: string;
}

export async function loadUpdateSources(): Promise<UpdateSources | null> {
  try {
    return await invoke<UpdateSources>("update_sources");
  } catch (e) {
    console.warn("[updater] failed to load update sources:", e);
    return null;
  }
}

/** 本体更新检查。未启用/占位源/网络失败 → null（静默）。 */
export async function checkAppUpdate(sources: UpdateSources | null): Promise<Update | null> {
  if (!sources?.app.enabled) return null;
  try {
    return await check();
  } catch (e) {
    console.info("[updater] app update check skipped:", e);
    return null;
  }
}

/** 插件更新检查。 */
export async function checkPluginUpdate(
  pluginId: string,
  currentVersion: string,
  target: string,
  sources: UpdateSources | null,
): Promise<PluginUpdateInfo | null> {
  const source = sources?.plugins[pluginId];
  if (!source?.enabled) return null;
  try {
    const res = await fetch(source.manifest, { method: "GET" });
    if (!res.ok) return null;
    const remote = (await res.json()) as RemotePluginManifest;
    if (!remote.version || compareSemver(remote.version, currentVersion) <= 0) return null;
    const file = remote.files?.[target];
    if (!file?.url || !file?.sha256) return null;
    return {
      pluginId,
      version: remote.version,
      notes: remote.notes,
      fileUrl: file.url,
      sha256: file.sha256,
      signature: file.signature,
    };
  } catch (e) {
    console.info(`[updater] plugin ${pluginId} update check skipped:`, e);
    return null;
  }
}

/** 下载并安装插件更新（Rust 侧校验 + 原子替换 + 热重载）。 */
export async function installPluginUpdate(info: PluginUpdateInfo): Promise<{
  report: { version: string; previous_version?: string | null; warnings: string[] };
}> {
  const res = await fetch(info.fileUrl, { method: "GET" });
  if (!res.ok) {
    throw new Error(`download failed (HTTP ${res.status})`);
  }
  const bytes = new Uint8Array(await res.arrayBuffer());
  const tmp = await join(
    await tempDir(),
    `zannen-${info.pluginId}-${Date.now()}.znplugin`,
  );
  await writeFile(tmp, bytes);
  return invoke("plugin_install", {
    pluginId: info.pluginId,
    archivePath: tmp,
    sha256: info.sha256,
    signature: info.signature ?? null,
    allowUnsigned: false,
  });
}

/** 宽松 semver 比较（major.minor.patch，忽略预发布段）。a>b→1，a<b→-1，相等→0。 */
export function compareSemver(a: string, b: string): number {
  const pa = a.split("-")[0].split(".").map((x) => parseInt(x, 10) || 0);
  const pb = b.split("-")[0].split(".").map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < 3; i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return d > 0 ? 1 : -1;
  }
  return 0;
}
