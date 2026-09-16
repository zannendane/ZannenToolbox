/**
 * 悬浮窗/视图指示灯状态机（纯函数，供 vitest 覆盖）。
 *
 * 优先级：error（红）> warn（黄，非致命告警闪烁窗口）> idle（暗灰）
 * > speaking（绿，随语音脉冲）> silent（灰，聆听中无语音）。
 */

import type { SessionState } from "./session";

export type DotKind = "idle" | "silent" | "speaking" | "warn" | "error";

/** 语音事件的最大有效窗口（超过未收到新事件视为不再说话）。 */
export const SPEAKING_WINDOW_MS = 700;
/** 非致命告警的黄色窗口时长。 */
export const WARN_WINDOW_MS = 3000;

export interface DotInput {
  state: SessionState;
  now: number;
  /** 最近一次音频事件时是否 speaking。 */
  speaking: boolean;
  /** 最近一次音频事件时间戳（无则为 0）。 */
  lastAudioAt: number;
  /** 黄色告警截止时间戳（无则过去时刻）。 */
  warnUntil: number;
}

export function dotKind({ state, now, speaking, lastAudioAt, warnUntil }: DotInput): DotKind {
  if (state === "error") return "error";
  if (now < warnUntil) return "warn";
  if (state === "idle") return "idle";
  if (speaking && now - lastAudioAt < SPEAKING_WINDOW_MS) return "speaking";
  return "silent";
}
