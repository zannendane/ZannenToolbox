import { describe, expect, it } from "vitest";
import {
  applyTranslation,
  applyUtterance,
  HISTORY_CAP,
  type HistoryEntry,
} from "./history";

describe("applyUtterance", () => {
  it("prepends newest first", () => {
    let list: HistoryEntry[] = [];
    list = applyUtterance(list, { id: 1, text: "hello", lang: "en" });
    list = applyUtterance(list, { id: 2, text: "world", lang: "en" });
    expect(list.map((e) => e.id)).toEqual([2, 1]);
    expect(list[0]).toMatchObject({ source: "world", translation: null });
  });

  it("dedupes by id", () => {
    let list: HistoryEntry[] = [];
    list = applyUtterance(list, { id: 1, text: "a" });
    list = applyUtterance(list, { id: 1, text: "b" });
    expect(list).toHaveLength(1);
    expect(list[0].source).toBe("b");
  });

  it("caps the list", () => {
    let list: HistoryEntry[] = [];
    for (let i = 1; i <= HISTORY_CAP + 5; i += 1) {
      list = applyUtterance(list, { id: i, text: `t${i}` });
    }
    expect(list).toHaveLength(HISTORY_CAP);
    expect(list[0].id).toBe(HISTORY_CAP + 5);
    expect(list[list.length - 1].id).toBe(6);
  });
});

describe("applyTranslation", () => {
  it("completes the matching entry", () => {
    let list: HistoryEntry[] = [];
    list = applyUtterance(list, { id: 7, text: "bonjour", lang: "en" });
    list = applyTranslation(list, { id: 7, text: "你好", source: "bonjour", target: "zh" });
    expect(list[0]).toMatchObject({ translation: "你好", target: "zh" });
  });

  it("creates a fallback entry when utterance was missed", () => {
    const list = applyTranslation([], { id: 3, text: "x", source: "s", target: "en" });
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ source: "s", translation: "x", target: "en" });
  });
});
