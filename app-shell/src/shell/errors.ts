/**
 * 错误识别码（壳前端段）。完整目录见 docs/ERROR-CODES.md。
 *
 * 号段约定：
 * - E1xxx 壳/前端（装载、IPC、更新）
 * - E2xxx 插件 ABI/管理器
 * - E3xxx 硬件服务（serial/ble/uf2/dfu）
 * - E4xxx 更新与安装
 */

export const SHELL_ERR = {
  PLUGIN_FETCH_FAILED: "E1101",
  PLUGIN_IMPORT_FAILED: "E1102",
  PLUGIN_NO_MODULE: "E1103",
  PLUGIN_RENDER_FAILED: "E1104",
  PLUGIN_MANIFEST_LIST_FAILED: "E1105",
  APP_UPDATE_FAILED: "E1201",
  PLUGIN_UPDATE_FAILED: "E1202",
} as const;

export type ShellErrCode = (typeof SHELL_ERR)[keyof typeof SHELL_ERR];

/** 带识别码的错误：message 形如 "[E1101] …"，UI 直接展示即可。 */
export function codedError(code: ShellErrCode, detail: string): Error {
  return new Error(`[${code}] ${detail}`);
}

/** 从任意异常提取展示文本（已带码的透传）。 */
export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
