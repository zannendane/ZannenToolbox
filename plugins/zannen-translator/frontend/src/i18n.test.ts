import { describe, expect, it } from "vitest";
import { t, tf, type I18nKey } from "./i18n";

describe("i18n dictionaries", () => {
  it("resolves core keys in both locales", () => {
    const keys: I18nKey[] = ["plugin.name", "translator.title", "session.start", "overlay.open"];
    for (const key of keys) {
      expect(t("zh-CN", key)).not.toBe(key);
      expect(t("en-US", key)).not.toBe(key);
      expect(t("zh-CN", key)).not.toBe(t("en-US", key));
    }
  });

  it("plugin.name matches the registered titles", () => {
    expect(t("zh-CN", "plugin.name")).toBe("Zannen 实时翻译");
    expect(t("en-US", "plugin.name")).toBe("Zannen Live Translate");
  });

  it("tf substitutes placeholders", () => {
    expect(tf("en-US", "session.startFailed", { error: "boom" })).toBe("Failed to start: boom");
    expect(tf("zh-CN", "session.startFailed", { error: "boom" })).toContain("boom");
  });
});
