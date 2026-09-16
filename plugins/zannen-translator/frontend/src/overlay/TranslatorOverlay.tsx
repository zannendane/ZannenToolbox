/**
 * 悬浮窗 overlay 组件：横向矩形，从上到下 =
 * 工具栏（拖动区/状态点/置顶/关闭）→ 输入源 STT 区 → 翻译输出区。
 *
 * 订阅 zannen.translator/* 总线事件（与主视图共享后端会话）；
 * 玻璃拟态半透明，窗口本体透明（壳 overlay_open 创建）。
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { Badge, useBusEvent, useLocale } from "@zannen/plugin-sdk";
import { Pin, PinOff, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { TranslationPayload, UtterancePayload } from "../history";
import { t } from "../i18n";
import {
  dotKind,
  WARN_WINDOW_MS,
} from "../indicator";
import {
  ERR_AUDIO_DEVICE,
  STATE_LABEL,
  TOPIC_AUDIO,
  TOPIC_ERROR,
  TOPIC_PARTIAL,
  TOPIC_PARTIAL_TRANSLATION,
  TOPIC_PREFIX,
  TOPIC_STATE,
  TOPIC_TRANSLATION,
  TOPIC_UTTERANCE,
  type ErrorPayload,
  type SessionState,
  type StatePayload,
} from "../session";

interface LatestEntry {
  id: number;
  source: string;
  lang: string | null;
  translation: string | null;
  target: string | null;
}

export function TranslatorOverlay() {
  const locale = useLocale();
  const [sessionState, setSessionState] = useState<SessionState>("idle");
  const [latest, setLatest] = useState<LatestEntry | null>(null);
  // 实时 STT 的滚动中间结果（utterance 到达时清除）
  const [partial, setPartial] = useState<string | null>(null);
  // 实时翻译中间稿（逐词翻译 + 整体纠正；最终译文到达时清除）
  const [partialTr, setPartialTr] = useState<string | null>(null);
  // 指示灯输入：语音活动 + 非致命告警窗口（黄）
  const [speaking, setSpeaking] = useState(false);
  const [lastAudioAt, setLastAudioAt] = useState(0);
  const [warnUntil, setWarnUntil] = useState(0);
  // 指示灯衰减 tick（语音停 / 告警窗口过期后自动翻色）
  const [nowTick, setNowTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setNowTick(Date.now()), 300);
    return () => clearInterval(timer);
  }, []);
  // 窗口创建即 always_on_top(true)，本地状态与之对应
  const [pinned, setPinned] = useState(true);
  const win = useMemo(() => getCurrentWindow(), []);

  const topicFilter = useCallback((topic: string) => topic.startsWith(TOPIC_PREFIX), []);
  useBusEvent<unknown>(topicFilter, (payload, topic) => {
    switch (topic) {
      case TOPIC_STATE:
        setSessionState((payload as StatePayload).state);
        break;
      case TOPIC_UTTERANCE: {
        const p = payload as UtterancePayload;
        setPartial(null);
        setLatest({
          id: p.id,
          source: p.text,
          lang: p.lang ?? null,
          translation: null,
          target: null,
        });
        break;
      }
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
      case TOPIC_ERROR: {
        const p = payload as ErrorPayload;
        // 致命设备错误经 state=error 转红；其余（STT/翻译失败、迟缓）转黄 3s
        if (p.code !== ERR_AUDIO_DEVICE) setWarnUntil(Date.now() + WARN_WINDOW_MS);
        break;
      }
      case TOPIC_TRANSLATION: {
        setPartialTr(null);
        const p = payload as TranslationPayload;
        setLatest((cur) =>
          cur && cur.id === p.id ? { ...cur, translation: p.text, target: p.target } : cur,
        );
        break;
      }
      default:
        break;
    }
  });

  const togglePin = () => {
    const next = !pinned;
    setPinned(next);
    void win.setAlwaysOnTop(next).catch(() => setPinned(!next));
  };

  return (
    <div className="ztr-overlay-root">
      {/* 工具栏：左侧为拖动区（data-tauri-drag-region），右侧为窗口操作 */}
      <div className="ztr-overlay-toolbar">
        <div className="ztr-overlay-drag" data-tauri-drag-region>
          <span
            className={`ztr-overlay-dot is-${dotKind({
              state: sessionState,
              now: nowTick,
              speaking,
              lastAudioAt,
              warnUntil,
            })}`}
            data-tauri-drag-region
          />
          {/* 状态灯文字（与主窗口状态徽标同源） */}
          <span className="ztr-overlay-state" data-tauri-drag-region>
            {t(locale, STATE_LABEL[sessionState])}
          </span>
          <span className="ztr-overlay-title" data-tauri-drag-region>
            {t(locale, "plugin.name")}
          </span>
        </div>
        <button
          type="button"
          className={`ztr-overlay-btn${pinned ? " is-on" : ""}`}
          title={t(locale, pinned ? "overlay.unpin" : "overlay.pin")}
          onClick={togglePin}
        >
          {pinned ? <Pin size={13} /> : <PinOff size={13} />}
        </button>
        <button
          type="button"
          className="ztr-overlay-btn is-close"
          title={t(locale, "overlay.close")}
          onClick={() => void win.close()}
        >
          <X size={13} />
        </button>
      </div>

      <div className="ztr-overlay-panes">
        {/* 输入源 STT 区 */}
        <section className="ztr-overlay-pane">
          <header className="ztr-overlay-pane-head">
            <Badge tone="accent">{latest?.lang ? latest.lang.toUpperCase() : "…"}</Badge>
            {sessionState === "processing" ? (
              <span className="ztr-overlay-progress">{t(locale, "overlay.processing")}</span>
            ) : null}
          </header>
          <div
            className={`ztr-overlay-text${partial ? " is-partial" : latest ? "" : " is-placeholder"}`}
          >
            {partial ?? latest?.source ?? t(locale, "overlay.sourcePlaceholder")}
          </div>
        </section>

        {/* 翻译输出区 */}
        <section className="ztr-overlay-pane">
          <header className="ztr-overlay-pane-head">
            <Badge tone="ok">{latest?.target ? latest.target.toUpperCase() : "→"}</Badge>
          </header>
          <div
            className={`ztr-overlay-text${partialTr ? " is-partial" : latest?.translation ? "" : " is-placeholder"}`}
          >
            {partialTr ?? latest?.translation ?? t(locale, "overlay.translationPlaceholder")}
          </div>
        </section>
      </div>
    </div>
  );
}
