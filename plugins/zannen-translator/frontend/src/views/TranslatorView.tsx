/**
 * 实时翻译主视图：STT/翻译提供源配置、会话控制、悬浮窗入口、会话历史。
 *
 * 权限前置告知（macOS）：首次「开始」真实 STT 前弹说明模态框（localStorage 记忆）；
 * 收到 E3601（设备不可用/权限被拒）时展示权限横幅 + 系统设置入口。
 */

import {
  Badge,
  Button,
  EmptyState,
  GlassCard,
  SectionTitle,
  invokePlugin,
  invokeShell,
  useBusEvent,
  useLocale,
} from "@zannen/plugin-sdk";
import { ExternalLink, Languages, Mic, MicOff, Play, Square } from "lucide-react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  INPUT_LANGUAGES,
  TARGET_LANGUAGES,
  loadConfig,
  saveConfig,
  toStartArgs,
  PLUGIN_ID,
  STT_PROVIDER_PRESETS,
  TRANSLATE_PROVIDER_PRESETS,
  loadProfiles,
  saveProfiles,
  upsertProfile,
  removeProfile,
  type ConfigProfile,
  type InputLanguage,
  type SttConfig,
  type TargetLanguage,
  type TranslateConfig,
  type TranslatorConfig,
} from "../config";
import { applyTranslation, applyUtterance, type HistoryEntry, type TranslationPayload, type UtterancePayload } from "../history";
import { t, tf } from "../i18n";
import {
  dotKind,
  WARN_WINDOW_MS,
} from "../indicator";
import {
  ERR_AUDIO_DEVICE,
  TOPIC_AUDIO,
  TOPIC_ERROR,
  TOPIC_PARTIAL_TRANSLATION,
  TOPIC_PARTIAL,
  TOPIC_PREFIX,
  TOPIC_STATE,
  TOPIC_TRANSLATION,
  TOPIC_UTTERANCE,
  type ErrorPayload,
  STATE_LABEL,
  type SessionState,
  type StatePayload,
} from "../session";

/** macOS 麦克风授权前置告知记忆键。 */
const MIC_ACK_KEY = "zannen.translator.mic-ack";
const isMac = navigator.userAgent.includes("Mac OS");

const STATE_TONE: Record<SessionState, "muted" | "ok" | "accent" | "critical"> = {
  idle: "muted",
  listening: "ok",
  processing: "accent",
  error: "critical",
};

/** 语言码显示名（语言名按本土化惯例书写，属展示数据）。 */
const LANG_DISPLAY: Record<string, string> = {
  en: "English",
  ja: "日本語",
  zh: "中文",
};

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="ztr-field">
      <span className="ztr-field-label">{label}</span>
      {children}
      {hint ? <span className="ztr-field-hint">{hint}</span> : null}
    </label>
  );
}

export function TranslatorView() {
  const locale = useLocale();
  const [config, setConfig] = useState<TranslatorConfig>(() => loadConfig());
  const [active, setActive] = useState(false);
  const [sessionState, setSessionState] = useState<SessionState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [micDenied, setMicDenied] = useState<string | null>(null);
  const [micAcked, setMicAcked] = useState(() => localStorage.getItem(MIC_ACK_KEY) === "1");
  const [showMicNotice, setShowMicNotice] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  // 实时 STT 的滚动中间结果（utterance 到达时清除，不进历史）
  const [partial, setPartial] = useState<string | null>(null);
  // 实时翻译中间稿（逐词翻译 + 整体纠正；最终译文到达时清除）
  const [partialTr, setPartialTr] = useState<string | null>(null);
  // 指示灯输入：语音活动 + 非致命告警窗口（黄）
  const [speaking, setSpeaking] = useState(false);
  const [lastAudioAt, setLastAudioAt] = useState(0);
  const [warnUntil, setWarnUntil] = useState(0);
  const [nowTick, setNowTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setNowTick(Date.now()), 300);
    return () => clearInterval(timer);
  }, []);

  // 配置持久化（zannen.translator.config）
  useEffect(() => saveConfig(config), [config]);

  // 命名配置方案：保存/应用/删除（zannen.translator.profiles）
  const [profiles, setProfiles] = useState<ConfigProfile[]>(() => loadProfiles());
  const [profileName, setProfileName] = useState("");

  const persistProfiles = (next: ConfigProfile[]) => {
    setProfiles(next);
    saveProfiles(next);
  };
  const saveCurrentAsProfile = () => {
    const name = profileName.trim();
    if (!name) return;
    persistProfiles(upsertProfile(profiles, name, config));
    setProfileName("");
  };
  const applyNamedProfile = (name: string) => {
    const p = profiles.find((p) => p.name === name);
    if (p) setConfig(structuredClone(p.config));
  };

  const updateStt = (patch: Partial<SttConfig>) =>
    setConfig((c) => ({ ...c, stt: { ...c.stt, ...patch } }));
  const updateTranslate = (patch: Partial<TranslateConfig>) =>
    setConfig((c) => ({ ...c, translate: { ...c.translate, ...patch } }));

  // 总线事件：状态机 / 历史 / 错误（E3601 → 权限横幅）
  const topicFilter = useCallback((topic: string) => topic.startsWith(TOPIC_PREFIX), []);
  useBusEvent<unknown>(topicFilter, (payload, topic) => {
    switch (topic) {
      case TOPIC_STATE: {
        const p = payload as StatePayload;
        setSessionState(p.state);
        if (p.state === "idle") setActive(false);
        break;
      }
      case TOPIC_UTTERANCE:
        setPartial(null);
        setHistory((h) => applyUtterance(h, payload as UtterancePayload));
        break;
      case TOPIC_PARTIAL:
        setPartial((payload as { text?: string }).text ?? null);
        break;
      case TOPIC_PARTIAL_TRANSLATION:
        setPartialTr((payload as { text?: string }).text ?? null);
        break;
      case TOPIC_AUDIO: {
        const p = payload as { level?: number; speaking?: boolean };
        setSpeaking(p.speaking ?? false);
        setLastAudioAt(Date.now());
        break;
      }
      case TOPIC_TRANSLATION:
        setPartialTr(null);
        setHistory((h) => applyTranslation(h, payload as TranslationPayload));
        break;
      case TOPIC_ERROR: {
        const p = payload as ErrorPayload;
        const text = `[${p.code ?? "E3600"}] ${p.message ?? "unknown error"}`;
        if (p.code === ERR_AUDIO_DEVICE) {
          setMicDenied(text);
        } else {
          // 非致命错误：不刷屏到错误条，仅指示灯转黄 3s
          setWarnUntil(Date.now() + WARN_WINDOW_MS);
        }
        break;
      }
      default:
        break;
    }
  });

  // 挂载时同步后端会话现状（如悬浮窗开启期间会话已在运行）
  useEffect(() => {
    invokePlugin<{ active: boolean; state: SessionState }>(PLUGIN_ID, "session.status")
      .then((s) => {
        setActive(s.active);
        if (s.state) setSessionState(s.state);
      })
      .catch(() => {});
  }, []);

  const doStart = useCallback(async () => {
    setError(null);
    try {
      await invokePlugin(PLUGIN_ID, "session.start", toStartArgs(config));
      setActive(true);
      setSessionState("listening");
      setMicDenied(null);
    } catch (e) {
      const msg = String(e);
      if (msg.includes(ERR_AUDIO_DEVICE)) {
        setMicDenied(msg);
      } else {
        setError(tf(locale, "session.startFailed", { error: msg }));
      }
    }
  }, [config, locale]);

  /** 「开始」入口：真实 STT 且 macOS 未确认告知 → 先弹说明模态框。 */
  const onStartClick = () => {
    if (isMac && !micAcked && config.stt.provider !== "mock") {
      setShowMicNotice(true);
      return;
    }
    void doStart();
  };

  const allowMic = () => {
    localStorage.setItem(MIC_ACK_KEY, "1");
    setMicAcked(true);
    setShowMicNotice(false);
    void doStart(); // 系统授权框将随后出现
  };

  const doStop = () => {
    void invokePlugin(PLUGIN_ID, "session.stop").catch(() => {});
    setActive(false);
    setSessionState("idle");
  };

  const openOverlay = () => {
    invokeShell("overlay_open", { plugin: PLUGIN_ID }).catch((e) =>
      setError(tf(locale, "overlay.openFailed", { error: String(e) })),
    );
  };

  const openMicSettings = () =>
    void invokeShell("open_permission_settings", { kind: "microphone" }).catch((e) =>
      setError(String(e)),
    );

  const sttMock = config.stt.provider === "mock";
  const trMock = config.translate.provider === "mock";
  const trChat = config.translate.provider === "openai-compatible";

  return (
    <div className="ztr-view">
      <SectionTitle title={t(locale, "translator.title")} desc={t(locale, "translator.desc")} />

      {/* ---------- STT 提供源 ---------- */}
      <GlassCard>
        <div className="ztr-card-title">{t(locale, "stt.cardTitle")}</div>
        <div className="ztr-form">
          <Field label={t(locale, "stt.provider")}>
            <select
              className="ztr-select"
              value={config.stt.provider}
              disabled={active}
              onChange={(e) =>
                updateStt({
                  provider: e.target.value as SttConfig["provider"],
                  ...STT_PROVIDER_PRESETS[e.target.value as SttConfig["provider"]],
                })
              }
            >
              <option value="mock">{t(locale, "stt.provider.mock")}</option>
              <option value="openai">{t(locale, "stt.provider.openai")}</option>
              <option value="dashscope">{t(locale, "stt.provider.dashscope")}</option>
              <option value="dashscope-realtime">{t(locale, "stt.provider.dashscope-realtime")}</option>
            </select>
          </Field>
          <Field label={t(locale, "stt.language")}>
            <select
              className="ztr-select"
              value={config.stt.language}
              disabled={active || sttMock}
              onChange={(e) => updateStt({ language: e.target.value as InputLanguage })}
            >
              {INPUT_LANGUAGES.map((l) => (
                <option key={l} value={l}>
                  {l === "auto" ? t(locale, "stt.language.auto") : LANG_DISPLAY[l]}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t(locale, "stt.endpoint")} hint={sttMock ? undefined : t(locale, "stt.endpointHint")}>
            <input
              className="ztr-input"
              type="text"
              spellCheck={false}
              value={config.stt.endpoint}
              disabled={active || sttMock}
              onChange={(e) => updateStt({ endpoint: e.target.value })}
            />
          </Field>
          <Field label={t(locale, "stt.apiKey")}>
            <input
              className="ztr-input"
              type="password"
              spellCheck={false}
              autoComplete="off"
              value={config.stt.apiKey}
              disabled={active || sttMock}
              onChange={(e) => updateStt({ apiKey: e.target.value })}
            />
          </Field>
          <Field label={t(locale, "stt.model")}>
            <input
              className="ztr-input"
              type="text"
              spellCheck={false}
              value={config.stt.model}
              disabled={active || sttMock}
              onChange={(e) => updateStt({ model: e.target.value })}
            />
          </Field>
        </div>
      </GlassCard>

      {/* ---------- 翻译提供源 ---------- */}
      <GlassCard>
        <div className="ztr-card-title">{t(locale, "translate.cardTitle")}</div>
        <div className="ztr-form">
          <Field label={t(locale, "translate.provider")}>
            <select
              className="ztr-select"
              value={config.translate.provider}
              disabled={active}
              onChange={(e) =>
                updateTranslate({
                  provider: e.target.value as TranslateConfig["provider"],
                  ...TRANSLATE_PROVIDER_PRESETS[e.target.value as TranslateConfig["provider"]],
                })
              }
            >
              <option value="mock">{t(locale, "translate.provider.mock")}</option>
              <option value="openai-compatible">
                {t(locale, "translate.provider.openai-compatible")}
              </option>
              <option value="deepl">{t(locale, "translate.provider.deepl")}</option>
              <option value="dashscope">{t(locale, "translate.provider.dashscope")}</option>
            </select>
          </Field>
          <Field label={t(locale, "translate.target")}>
            <select
              className="ztr-select"
              value={config.translate.target}
              disabled={active}
              onChange={(e) => updateTranslate({ target: e.target.value as TargetLanguage })}
            >
              {TARGET_LANGUAGES.map((l) => (
                <option key={l} value={l}>
                  {LANG_DISPLAY[l]}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t(locale, "stt.endpoint")}>
            <input
              className="ztr-input"
              type="text"
              spellCheck={false}
              value={config.translate.endpoint}
              disabled={active || trMock}
              onChange={(e) => updateTranslate({ endpoint: e.target.value })}
            />
          </Field>
          <Field label={t(locale, "stt.apiKey")}>
            <input
              className="ztr-input"
              type="password"
              spellCheck={false}
              autoComplete="off"
              value={config.translate.apiKey}
              disabled={active || trMock}
              onChange={(e) => updateTranslate({ apiKey: e.target.value })}
            />
          </Field>
          <Field label={t(locale, "stt.model")}>
            <input
              className="ztr-input"
              type="text"
              spellCheck={false}
              value={config.translate.model}
              disabled={active || trMock || !trChat}
              onChange={(e) => updateTranslate({ model: e.target.value })}
            />
          </Field>
        </div>
      </GlassCard>

      {/* ---------- 配置方案（多套 API 配置保存/切换） ---------- */}
      <GlassCard>
        <div className="ztr-card-title">{t(locale, "profiles.cardTitle")}</div>
        <div className="ztr-form">
          <Field label={t(locale, "profiles.save")}>
            <div className="ztr-profile-row">
              <input
                className="ztr-input"
                type="text"
                spellCheck={false}
                placeholder={t(locale, "profiles.placeholder")}
                value={profileName}
                disabled={active}
                onChange={(e) => setProfileName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveCurrentAsProfile();
                }}
              />
              <Button
                variant="ghost"
                disabled={active || profileName.trim() === ""}
                onClick={saveCurrentAsProfile}
              >
                {t(locale, "profiles.save")}
              </Button>
            </div>
          </Field>
          <Field label={t(locale, "profiles.cardTitle")}>
            <div className="ztr-profile-row">
              <select
                className="ztr-select"
                value=""
                disabled={active || profiles.length === 0}
                onChange={(e) => {
                  if (e.target.value) applyNamedProfile(e.target.value);
                }}
              >
                <option value="">
                  {profiles.length === 0
                    ? t(locale, "profiles.empty")
                    : t(locale, "profiles.select")}
                </option>
                {profiles.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
          </Field>
          {profiles.length > 0 && (
            <div className="ztr-profile-chips">
              {profiles.map((p) => (
                <span key={p.name} className="ztr-profile-chip">
                  <button
                    type="button"
                    className="ztr-profile-chip-apply"
                    disabled={active}
                    onClick={() => applyNamedProfile(p.name)}
                  >
                    {p.name}
                  </button>
                  <button
                    type="button"
                    className="ztr-profile-chip-del"
                    title={t(locale, "profiles.delete")}
                    disabled={active}
                    onClick={() => persistProfiles(removeProfile(profiles, p.name))}
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}
        </div>
      </GlassCard>

      {/* ---------- 会话控制 ---------- */}
      <div className="ztr-controls">
        {active ? (
          <Button variant="danger" onClick={doStop}>
            <Square size={13} /> {t(locale, "session.stop")}
          </Button>
        ) : (
          <Button variant="primary" onClick={onStartClick}>
            <Play size={13} /> {t(locale, "session.start")}
          </Button>
        )}
        <Badge tone={STATE_TONE[sessionState]}>
          <span
            className={`ztr-dot is-${dotKind({
              state: sessionState,
              now: nowTick,
              speaking,
              lastAudioAt,
              warnUntil,
            })}`}
          />
          {t(locale, STATE_LABEL[sessionState])}
        </Badge>
        <span className="ztr-controls-spacer" />
        <Button variant="ghost" onClick={openOverlay}>
          <ExternalLink size={13} /> {t(locale, "overlay.open")}
        </Button>
      </div>

      {error && <div className="ztr-error">{error}</div>}

      {/* 麦克风权限被拒横幅：说明 + 重试 + 系统设置入口 */}
      {micDenied && (
        <GlassCard className="ztr-perm-notice">
          <div className="ztr-perm-notice-head">
            <MicOff size={16} />
            <span>{t(locale, "micDenied.title")}</span>
          </div>
          <p className="ztr-perm-notice-desc">{t(locale, "micDenied.desc")}</p>
          <p className="ztr-perm-notice-reasons">{t(locale, "micDenied.reasons")}</p>
          <div className="ztr-perm-notice-code">{micDenied}</div>
          <div className="ztr-perm-notice-actions">
            <Button variant="primary" onClick={() => void doStart()}>
              {t(locale, "micDenied.retry")}
            </Button>
            <Button variant="ghost" onClick={openMicSettings}>
              {t(locale, "micDenied.openSettings")}
            </Button>
          </div>
        </GlassCard>
      )}

      {/* ---------- 会话历史 ---------- */}
      <GlassCard>
        <div className="ztr-card-title">{t(locale, "history.title")}</div>
        {partial && <div className="ztr-partial-line">{partial}</div>}
        {partialTr && <div className="ztr-partial-line is-translation">{partialTr}</div>}
        {history.length === 0 ? (
          <EmptyState
            icon={<Languages size={24} />}
            title={t(locale, "history.empty")}
          />
        ) : (
          <div className="ztr-history">
            {history.map((e) => (
              <div key={e.id} className="ztr-history-item">
                <div className="ztr-history-line">
                  {e.lang ? <Badge tone="accent">{e.lang.toUpperCase()}</Badge> : null}
                  <span className="ztr-history-text">{e.source}</span>
                </div>
                <div className="ztr-history-line is-translation">
                  {e.target ? <Badge tone="ok">{e.target.toUpperCase()}</Badge> : null}
                  {e.passthrough ? (
                    <Badge tone="muted">{t(locale, "history.passthrough")}</Badge>
                  ) : null}
                  <span className="ztr-history-text">
                    {e.translation ?? t(locale, "history.pending")}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </GlassCard>

      {/* 麦克风授权前置告知（macOS，系统授权框前给上下文） */}
      {showMicNotice && (
        <div className="ztr-modal-overlay" onClick={() => setShowMicNotice(false)}>
          <div className="ztr-modal zt-glass" onClick={(e) => e.stopPropagation()}>
            <div className="ztr-modal-icon">
              <Mic size={22} />
            </div>
            <div className="ztr-modal-title">{t(locale, "micNotice.title")}</div>
            <p className="ztr-modal-desc">{t(locale, "micNotice.desc")}</p>
            <div className="ztr-modal-actions">
              <Button variant="primary" onClick={allowMic}>
                {t(locale, "micNotice.allow")}
              </Button>
              <Button variant="ghost" onClick={() => setShowMicNotice(false)}>
                {t(locale, "micNotice.later")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
