/**
 * 会话领域共享常量：总线 topic 前缀与会话状态机。
 */

export const TOPIC_PREFIX = "zannen.translator/";
export const TOPIC_STATE = "zannen.translator/state";
export const TOPIC_UTTERANCE = "zannen.translator/utterance";
export const TOPIC_TRANSLATION = "zannen.translator/translation";
export const TOPIC_ERROR = "zannen.translator/error";
export const TOPIC_PARTIAL = "zannen.translator/partial";
export const TOPIC_PARTIAL_TRANSLATION = "zannen.translator/partial_translation";
/** 音频电平/语音活动（指示灯用）：{level: number, speaking: boolean}。 */
export const TOPIC_AUDIO = "zannen.translator/audio";

export type SessionState = "idle" | "listening" | "processing" | "error";

export interface StatePayload {
  state: SessionState;
  detail?: string;
}

export interface ErrorPayload {
  code?: string;
  message?: string;
}

/** 音频输入设备错误码（权限被拒同码，前端据此展示权限横幅）。 */
export const ERR_AUDIO_DEVICE = "E3601";
/** 提供源响应迟缓（非致命，指示灯转黄）。 */
export const ERR_SLOW = "E3605";

/** 会话状态 → i18n 键（悬浮窗/主视图共用）。 */
export const STATE_LABEL = {
  idle: "session.state.idle",
  listening: "session.state.listening",
  processing: "session.state.processing",
  error: "session.state.error",
} as const;
