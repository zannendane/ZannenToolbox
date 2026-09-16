import { describe, expect, it } from "vitest";
import {
  DEFAULT_CONFIG,
  parseConfig,
  serializeConfig,
  toStartArgs,
} from "./config";

describe("parseConfig", () => {
  it("falls back to defaults on null / garbage", () => {
    expect(parseConfig(null)).toEqual(DEFAULT_CONFIG);
    expect(parseConfig("")).toEqual(DEFAULT_CONFIG);
    expect(parseConfig("{not json")).toEqual(DEFAULT_CONFIG);
    expect(parseConfig("42")).toEqual(DEFAULT_CONFIG);
  });

  it("merges partial config with defaults", () => {
    const cfg = parseConfig(
      JSON.stringify({ stt: { provider: "openai", apiKey: "sk-x" } }),
    );
    expect(cfg.stt.provider).toBe("openai");
    expect(cfg.stt.apiKey).toBe("sk-x");
    expect(cfg.stt.model).toBe(DEFAULT_CONFIG.stt.model);
    expect(cfg.translate.provider).toBe("mock");
  });

  it("rejects unknown enum values back to defaults", () => {
    const cfg = parseConfig(
      JSON.stringify({
        stt: { provider: "whisperx", language: "english" },
        translate: { provider: "bing", target: "fr" },
      }),
    );
    expect(cfg.stt.provider).toBe("mock");
    expect(cfg.stt.language).toBe("auto");
    expect(cfg.translate.provider).toBe("mock");
    expect(cfg.translate.target).toBe("zh");
  });

  it("serialize/parse roundtrips", () => {
    const cfg = parseConfig(null);
    cfg.stt.provider = "openai";
    cfg.stt.language = "ja";
    cfg.translate.provider = "deepl";
    cfg.translate.target = "en";
    expect(parseConfig(serializeConfig(cfg))).toEqual(cfg);
  });
});

describe("toStartArgs", () => {
  it("maps to backend arg shape (camelCase apiKey)", () => {
    const cfg = parseConfig(null);
    cfg.stt.apiKey = "k1";
    cfg.translate.apiKey = "k2";
    const args = toStartArgs(cfg) as {
      stt: Record<string, unknown>;
      translate: Record<string, unknown>;
    };
    expect(args.stt).toMatchObject({
      provider: "mock",
      apiKey: "k1",
      language: "auto",
    });
    expect(args.translate).toMatchObject({
      provider: "mock",
      apiKey: "k2",
      target: "zh",
    });
  });
});

describe("config profiles", () => {
  it("parses stored profiles tolerantly", async () => {
    const { parseProfiles } = await import("./config");
    expect(parseProfiles(null)).toEqual([]);
    expect(parseProfiles("garbage")).toEqual([]);
    expect(parseProfiles("{}")).toEqual([]);
    const raw = JSON.stringify([
      { name: "qwen", config: { stt: { provider: "dashscope", apiKey: "sk-1" } } },
      { name: "", config: {} },
      "junk",
    ]);
    const profiles = parseProfiles(raw);
    expect(profiles).toHaveLength(1);
    expect(profiles[0].name).toBe("qwen");
    expect(profiles[0].config.stt.provider).toBe("dashscope");
    expect(profiles[0].config.stt.apiKey).toBe("sk-1");
    // 缺失字段按默认合并
    expect(profiles[0].config.translate.provider).toBe("mock");
  });

  it("upserts by name and removes", async () => {
    const { upsertProfile, removeProfile } = await import("./config");
    const base = parseConfig(null);
    let list = upsertProfile([], "a", base);
    expect(list).toHaveLength(1);
    const modified = parseConfig(null);
    modified.stt.apiKey = "sk-2";
    list = upsertProfile(list, "a", modified);
    expect(list).toHaveLength(1);
    expect(list[0].config.stt.apiKey).toBe("sk-2");
    list = upsertProfile(list, "b", base);
    expect(list).toHaveLength(2);
    // 深拷贝：后续改 base 不影响已存方案
    base.stt.apiKey = "mutated";
    expect(list[1].config.stt.apiKey).toBe("");
    expect(removeProfile(list, "a").map((p) => p.name)).toEqual(["b"]);
  });
});
