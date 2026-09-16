/**
 * 实时翻译会话配置：类型、默认值、localStorage 持久化（容错合并）。
 * 纯函数部分（serialize/parse/toStartArgs）供 vitest 覆盖。
 */

export const PLUGIN_ID = "zannen.translator";
export const CONFIG_KEY = "zannen.translator.config";

export type SttProvider = "mock" | "openai" | "dashscope" | "dashscope-realtime";
export type TranslateProvider = "mock" | "openai-compatible" | "deepl" | "dashscope";
/** 输入语言：auto 或 ISO 码。 */
export type InputLanguage = "auto" | "en" | "ja" | "zh";
/** 目标语言码。 */
export type TargetLanguage = "zh" | "en" | "ja";

export interface SttConfig {
  provider: SttProvider;
  endpoint: string;
  apiKey: string;
  model: string;
  language: InputLanguage;
}

export interface TranslateConfig {
  provider: TranslateProvider;
  endpoint: string;
  apiKey: string;
  model: string;
  target: TargetLanguage;
}

export interface TranslatorConfig {
  stt: SttConfig;
  translate: TranslateConfig;
}

export const DEFAULT_CONFIG: TranslatorConfig = {
  stt: {
    provider: "mock",
    endpoint: "https://api.openai.com/v1",
    apiKey: "",
    model: "whisper-1",
    language: "auto",
  },
  translate: {
    provider: "mock",
    endpoint: "https://api.openai.com/v1",
    apiKey: "",
    model: "gpt-4o-mini",
    target: "zh",
  },
};

export const STT_PROVIDERS: SttProvider[] = ["mock", "openai", "dashscope", "dashscope-realtime"];
export const TRANSLATE_PROVIDERS: TranslateProvider[] = [
  "mock",
  "openai-compatible",
  "deepl",
  "dashscope",
];
export const INPUT_LANGUAGES: InputLanguage[] = ["auto", "en", "ja", "zh"];
export const TARGET_LANGUAGES: TargetLanguage[] = ["zh", "en", "ja"];

/** 切换提供源时的端点/模型预填（用户仍可自由修改）。 */
export const STT_PROVIDER_PRESETS: Record<SttProvider, { endpoint: string; model: string }> = {
  mock: { endpoint: "https://api.openai.com/v1", model: "whisper-1" },
  openai: { endpoint: "https://api.openai.com/v1", model: "whisper-1" },
  dashscope: {
    endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen3-asr-flash",
  },
  "dashscope-realtime": {
    endpoint: "wss://dashscope.aliyuncs.com/api-ws/v1/inference",
    model: "qwen-audio-3.0-asr-flash-streaming",
  },
};

export const TRANSLATE_PROVIDER_PRESETS: Record<
  TranslateProvider,
  { endpoint: string; model: string }
> = {
  mock: { endpoint: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  "openai-compatible": { endpoint: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  deepl: { endpoint: "https://api-free.deepl.com", model: "" },
  dashscope: {
    endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-mt-turbo",
  },
};

function pick<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === "string" && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

function pickStr(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

export function serializeConfig(cfg: TranslatorConfig): string {
  return JSON.stringify(cfg);
}

/** 容错反序列化：字段缺失/类型不符/枚举越界一律回退默认值。 */
export function parseConfig(raw: string | null): TranslatorConfig {
  if (!raw) return structuredClone(DEFAULT_CONFIG);
  try {
    const v = JSON.parse(raw) as Partial<TranslatorConfig> | null;
    const d = DEFAULT_CONFIG;
    const stt = (v?.stt ?? {}) as Partial<SttConfig>;
    const tr = (v?.translate ?? {}) as Partial<TranslateConfig>;
    return {
      stt: {
        provider: pick(stt.provider, STT_PROVIDERS, d.stt.provider),
        endpoint: pickStr(stt.endpoint, d.stt.endpoint),
        apiKey: pickStr(stt.apiKey, d.stt.apiKey),
        model: pickStr(stt.model, d.stt.model),
        language: pick(stt.language, INPUT_LANGUAGES, d.stt.language),
      },
      translate: {
        provider: pick(tr.provider, TRANSLATE_PROVIDERS, d.translate.provider),
        endpoint: pickStr(tr.endpoint, d.translate.endpoint),
        apiKey: pickStr(tr.apiKey, d.translate.apiKey),
        model: pickStr(tr.model, d.translate.model),
        target: pick(tr.target, TARGET_LANGUAGES, d.translate.target),
      },
    };
  } catch {
    return structuredClone(DEFAULT_CONFIG);
  }
}

export function loadConfig(): TranslatorConfig {
  try {
    return parseConfig(localStorage.getItem(CONFIG_KEY));
  } catch {
    return structuredClone(DEFAULT_CONFIG);
  }
}

export function saveConfig(cfg: TranslatorConfig): void {
  try {
    localStorage.setItem(CONFIG_KEY, serializeConfig(cfg));
  } catch {
    // 隐私模式等写失败场景：配置仅存续于内存
  }
}

// ---------- 命名配置方案（多套 API 配置保存/切换） ----------

export const PROFILES_KEY = "zannen.translator.profiles";

export interface ConfigProfile {
  name: string;
  config: TranslatorConfig;
}

/** 容错解析方案列表：非法条目丢弃，配置部分走 parseConfig 同款合并。 */
export function parseProfiles(raw: string | null): ConfigProfile[] {
  if (!raw) return [];
  try {
    const v = JSON.parse(raw);
    if (!Array.isArray(v)) return [];
    return v
      .filter(
        (p): p is { name: string; config: Record<string, unknown> } =>
          !!p && typeof p === "object" && typeof p.name === "string" && p.name.trim() !== "",
      )
      .map((p) => ({
        name: p.name,
        config: parseConfig(JSON.stringify(p.config ?? null)),
      }));
  } catch {
    return [];
  }
}

export function loadProfiles(): ConfigProfile[] {
  try {
    return parseProfiles(localStorage.getItem(PROFILES_KEY));
  } catch {
    return [];
  }
}

export function saveProfiles(profiles: ConfigProfile[]): void {
  try {
    localStorage.setItem(PROFILES_KEY, JSON.stringify(profiles));
  } catch {
    // 同上：写失败仅存续于内存
  }
}

/** 新增/覆盖同名方案（返回新数组，不修改入参）。 */
export function upsertProfile(
  profiles: ConfigProfile[],
  name: string,
  config: TranslatorConfig,
): ConfigProfile[] {
  const trimmed = name.trim();
  const entry: ConfigProfile = { name: trimmed, config: structuredClone(config) };
  const rest = profiles.filter((p) => p.name !== trimmed);
  return [...rest, entry];
}

/** 按名删除方案（返回新数组）。 */
export function removeProfile(profiles: ConfigProfile[], name: string): ConfigProfile[] {
  return profiles.filter((p) => p.name !== name);
}

/** session.start 的后端参数（键名与后端 SttConfig/TranslateConfig::parse 对应）。 */
export function toStartArgs(cfg: TranslatorConfig): Record<string, unknown> {
  return {
    stt: {
      provider: cfg.stt.provider,
      endpoint: cfg.stt.endpoint,
      apiKey: cfg.stt.apiKey,
      model: cfg.stt.model,
      language: cfg.stt.language,
    },
    translate: {
      provider: cfg.translate.provider,
      endpoint: cfg.translate.endpoint,
      apiKey: cfg.translate.apiKey,
      model: cfg.translate.model,
      target: cfg.translate.target,
    },
  };
}
