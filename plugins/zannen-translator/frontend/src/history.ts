/**
 * 会话历史 reducer：utterance/translation 总线事件 → 历史条目列表。
 * 纯函数，供 vitest 覆盖；列表新条目在前，内存保留最近 HISTORY_CAP 条。
 */

export const HISTORY_CAP = 50;

export interface HistoryEntry {
  id: number;
  /** 识别原文。 */
  source: string;
  /** 识别语言码（auto 模式由 STT 回报；可能为 null）。 */
  lang: string | null;
  /** 译文（未到为 null）。 */
  translation: string | null;
  /** 目标语言码。 */
  target: string | null;
  /** 目标语言直通。 */
  passthrough: boolean;
}

export interface UtterancePayload {
  id: number;
  text: string;
  lang?: string | null;
}

export interface TranslationPayload {
  id: number;
  text: string;
  source: string;
  target: string;
  /** 目标语言直通（输入已是目标语言，未经翻译）。 */
  passthrough?: boolean;
}

function cap(list: HistoryEntry[], max: number): HistoryEntry[] {
  return list.length > max ? list.slice(0, max) : list;
}

/** STT 完成一段：新条目插入头部。 */
export function applyUtterance(
  list: HistoryEntry[],
  p: UtterancePayload,
  max: number = HISTORY_CAP,
): HistoryEntry[] {
  const entry: HistoryEntry = {
    id: p.id,
    source: p.text,
    lang: p.lang ?? null,
    translation: null,
    target: null,
    passthrough: false,
  };
  return cap([entry, ...list.filter((e) => e.id !== p.id)], max);
}

/** 翻译完成：按 id 补全条目；缺条目时以 source 兜底新建。 */
export function applyTranslation(
  list: HistoryEntry[],
  p: TranslationPayload,
  max: number = HISTORY_CAP,
): HistoryEntry[] {
  const hit = list.find((e) => e.id === p.id);
  if (!hit) {
    return cap(
      [
        {
          id: p.id,
          source: p.source,
          lang: null,
          translation: p.text,
          target: p.target,
          passthrough: p.passthrough ?? false,
        },
        ...list,
      ],
      max,
    );
  }
  return list.map((e) =>
    e.id === p.id
      ? { ...e, translation: p.text, target: p.target, passthrough: p.passthrough ?? false }
      : e,
  );
}
