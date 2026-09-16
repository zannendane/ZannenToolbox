/**
 * hex 工具：串口数据的文本化展示（纯函数，可单测）。
 */

/** hex 字符串 → UTF-8 文本（容错）。 */
export function hexToText(hex: string): string {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
}

/** hex 字符串 → 分组展示的 hex dump。 */
export function hexPretty(hex: string): string {
  return hex.replace(/(..)/g, "$1 ").trim();
}
