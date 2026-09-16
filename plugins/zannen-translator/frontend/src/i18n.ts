/**
 * 实时翻译 i18n：zh-CN 为基准字典，en-US 完整对照。
 * 视图用法：`const locale = useLocale(); t(locale, "session.start")`；
 * 含占位符的文案用 `tf(locale, "key", { error: "…" })`，占位符形如 `{error}`。
 */

import { createI18n, type Locale } from "@zannen/plugin-sdk";

const zhCN = {
  // ---------- 插件元信息（壳展示层本地化桥） ----------
  "plugin.name": "Zannen 实时翻译",
  "plugin.desc": "麦克风语音实时翻译：语音识别（STT）→ 目标语言翻译，支持悬浮窗",

  // ---------- 路由 ----------
  "translator.title": "实时翻译",
  "translator.desc": "麦克风输入 → 语音识别 → 翻译为目标语言；可打开悬浮窗常驻显示",

  // ---------- STT 配置 ----------
  "stt.cardTitle": "语音识别（STT）",
  "stt.provider": "提供源",
  "stt.provider.mock": "Mock（离线演示）",
  "stt.provider.openai": "OpenAI 兼容接口",
  "stt.provider.dashscope": "千问 DashScope（qwen3-asr）",
  "stt.provider.dashscope-realtime": "千问 DashScope 实时流式（WebSocket）",
  "stt.endpoint": "接口地址",
  "stt.apiKey": "API 密钥",
  "stt.model": "模型",
  "stt.language": "输入语言",
  "stt.language.auto": "自动识别",
  "stt.endpointHint": "可改为 Groq 或本地 whisper.cpp server 等兼容端点",

  // ---------- 翻译配置 ----------
  "translate.cardTitle": "翻译",
  "translate.provider": "提供源",
  "translate.provider.mock": "Mock（离线演示）",
  "translate.provider.openai-compatible": "OpenAI 兼容对话接口",
  "translate.provider.deepl": "DeepL",
  "translate.provider.dashscope": "千问 DashScope（qwen-mt）",
  "translate.target": "目标语言",

  // ---------- 配置方案 ----------
  "profiles.cardTitle": "配置方案",
  "profiles.placeholder": "方案名称（如：千问-日常）",
  "profiles.save": "保存当前配置",
  "profiles.select": "选择已保存方案…",
  "profiles.delete": "删除",
  "profiles.empty": "尚无已保存方案",

  // ---------- 会话控制 ----------
  "session.start": "开始",
  "session.stop": "停止",
  "session.state.idle": "空闲",
  "session.state.listening": "聆听中",
  "session.state.processing": "处理中",
  "session.state.error": "错误",
  "session.startFailed": "启动失败：{error}",
  "overlay.open": "打开悬浮窗",
  "overlay.openFailed": "打开悬浮窗失败：{error}",

  // ---------- 历史 ----------
  "history.title": "会话历史",
  "history.empty": "尚无识别记录 —— 点击「开始」并说话",
  "history.pending": "翻译中…",
  "history.passthrough": "原文直通",

  // ---------- 麦克风权限 ----------
  "micNotice.title": "需要麦克风权限",
  "micNotice.desc":
    "Zannen 实时翻译使用麦克风采集语音，用于识别与翻译。点击「继续并开始」后，系统将弹出麦克风授权框。",
  "micNotice.allow": "继续并开始",
  "micNotice.later": "暂不",
  "micDenied.title": "麦克风权限未授予",
  "micDenied.desc":
    "实时翻译需要麦克风采集语音。权限未授予期间无法开始会话，Mock 提供源不受影响。",
  "micDenied.reasons":
    "可能原因：在系统授权弹窗中选择了「不允许」；或此前在系统设置中关闭了本应用的麦克风权限。",
  "micDenied.retry": "重试",
  "micDenied.openSettings": "打开系统设置",

  // ---------- 悬浮窗 ----------
  "overlay.sourcePlaceholder": "等待语音…",
  "overlay.processing": "识别中…",
  "overlay.translationPlaceholder": "译文将显示在这里",
  "overlay.pin": "置顶",
  "overlay.unpin": "取消置顶",
  "overlay.close": "关闭",
};

const enUS: Record<keyof typeof zhCN, string> = {
  // ---------- plugin meta (shell display bridge) ----------
  "plugin.name": "Zannen Live Translate",
  "plugin.desc": "Real-time microphone speech translation: speech-to-text (STT) → target language, with floating overlay",

  // ---------- route ----------
  "translator.title": "Live Translate",
  "translator.desc": "Microphone → speech recognition → target-language translation; optional floating overlay",

  // ---------- STT config ----------
  "stt.cardTitle": "Speech recognition (STT)",
  "stt.provider": "Provider",
  "stt.provider.mock": "Mock (offline demo)",
  "stt.provider.openai": "OpenAI-compatible API",
  "stt.provider.dashscope": "Qwen DashScope (qwen3-asr)",
  "stt.provider.dashscope-realtime": "Qwen DashScope realtime (WebSocket)",
  "stt.endpoint": "Endpoint",
  "stt.apiKey": "API key",
  "stt.model": "Model",
  "stt.language": "Input language",
  "stt.language.auto": "Auto detect",
  "stt.endpointHint": "Point to Groq, a local whisper.cpp server, or any compatible endpoint",

  // ---------- translate config ----------
  "translate.cardTitle": "Translation",
  "translate.provider": "Provider",
  "translate.provider.mock": "Mock (offline demo)",
  "translate.provider.openai-compatible": "OpenAI-compatible chat API",
  "translate.provider.deepl": "DeepL",
  "translate.provider.dashscope": "Qwen DashScope (qwen-mt)",
  "translate.target": "Target language",

  // ---------- config profiles ----------
  "profiles.cardTitle": "Config profiles",
  "profiles.placeholder": "Profile name (e.g. Qwen daily)",
  "profiles.save": "Save current config",
  "profiles.select": "Select a saved profile...",
  "profiles.delete": "Delete",
  "profiles.empty": "No saved profiles yet",

  // ---------- session control ----------
  "session.start": "Start",
  "session.stop": "Stop",
  "session.state.idle": "Idle",
  "session.state.listening": "Listening",
  "session.state.processing": "Processing",
  "session.state.error": "Error",
  "session.startFailed": "Failed to start: {error}",
  "overlay.open": "Open overlay",
  "overlay.openFailed": "Failed to open overlay: {error}",

  // ---------- history ----------
  "history.title": "Session history",
  "history.empty": "Nothing yet — press Start and speak",
  "history.pending": "Translating…",
  "history.passthrough": "Passthrough",

  // ---------- microphone permission ----------
  "micNotice.title": "Microphone access needed",
  "micNotice.desc":
    "Zannen Live Translate uses the microphone to capture speech for recognition and translation. After you tap \"Continue & start\", the system will show a microphone permission prompt.",
  "micNotice.allow": "Continue & start",
  "micNotice.later": "Not now",
  "micDenied.title": "Microphone access not granted",
  "micDenied.desc":
    "Live translation needs the microphone to capture speech. Sessions cannot start while access is denied; the Mock provider is unaffected.",
  "micDenied.reasons":
    "Possible causes: \"Don't Allow\" was chosen in the system prompt, or microphone access for this app was turned off in system settings.",
  "micDenied.retry": "Retry",
  "micDenied.openSettings": "Open system settings",

  // ---------- overlay ----------
  "overlay.sourcePlaceholder": "Waiting for speech…",
  "overlay.processing": "Recognizing…",
  "overlay.translationPlaceholder": "Translation will appear here",
  "overlay.pin": "Pin on top",
  "overlay.unpin": "Unpin",
  "overlay.close": "Close",
};

export const t = createI18n({ "zh-CN": zhCN, "en-US": enUS });

export type I18nKey = keyof typeof zhCN;

/** 带占位符替换的翻译：`{name}` ← params.name。 */
export function tf(locale: Locale, key: I18nKey, params: Record<string, string | number>): string {
  let s = t(locale, key);
  for (const [k, v] of Object.entries(params)) {
    s = s.replaceAll(`{${k}}`, String(v));
  }
  return s;
}
