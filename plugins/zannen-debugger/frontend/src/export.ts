/**
 * 导出序列化：CSV（时间轴缓冲）与终端日志文本（纯函数，可单测）。
 */

import type { Buffers } from "./buffers";

export type Cell = number | string | null | undefined;

/** RFC4180 风格转义：含逗号/引号/换行的字段加引号并双写引号。 */
export function csvCell(v: Cell): string {
  if (v === null || v === undefined) return "";
  const s = String(v);
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** 通用 CSV 序列化：表头 + 行，LF 结尾。 */
export function toCsv(headers: string[], rows: Cell[][]): string {
  const lines = [headers.map(csvCell).join(",")];
  for (const row of rows) lines.push(row.map(csvCell).join(","));
  return lines.join("\n") + "\n";
}

/** IMU 缓冲 → CSV（ts_ms,ax,ay,az,gx,gy,gz）。 */
export function imuCsv(b: Buffers): string {
  const rows: Cell[][] = [];
  for (let i = 0; i < b.tImu.length; i++) {
    rows.push([b.tImu[i], b.ax[i], b.ay[i], b.az[i], b.gx[i], b.gy[i], b.gz[i]]);
  }
  return toCsv(["ts_ms", "ax", "ay", "az", "gx", "gy", "gz"], rows);
}

/** RF 缓冲 → CSV（ts_ms,rssi）。 */
export function rssiCsv(b: Buffers): string {
  const rows: Cell[][] = [];
  for (let i = 0; i < b.tRf.length; i++) rows.push([b.tRf[i], b.rssi[i]]);
  return toCsv(["ts_ms", "rssi"], rows);
}

/** 给导出基路径加后缀：`a/b.csv` → `a/b.imu.csv`（无 .csv 后缀则直接拼接）。 */
export function withSuffix(path: string, suffix: string): string {
  return path.replace(/\.csv$/i, "") + suffix;
}

export interface LogLine {
  dir: "rx" | "tx" | "sys";
  text: string;
  ts: string;
}

const DIR_TAGS: Record<LogLine["dir"], string> = { rx: "RX", tx: "TX", sys: "SYS" };

/** 终端行 → 导出文本；格式与屏幕显示一致（可选时间戳前缀）。 */
export function formatLogLines(lines: LogLine[], showTs: boolean): string {
  const out = lines.map((l) =>
    showTs ? `[${l.ts}] ${DIR_TAGS[l.dir]} ${l.text}` : `${DIR_TAGS[l.dir]} ${l.text}`,
  );
  return out.length === 0 ? "" : out.join("\n") + "\n";
}

/** 导出文件名时间戳：20260905-153000。 */
export function fileStamp(d = new Date()): string {
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}
