/**
 * 轻量 i18n：字典 + 语言订阅。
 *
 * - 壳持有语言状态（`zannen-locale` localStorage + navigator.language 默认），
 *   变化时派发 `zannen-locale` 自定义事件；插件与壳同一文档，天然同步。
 * - 插件用 `createI18n(dicts)` 生成自己的 `t()`，配合 `useLocale()` 订阅。
 */

import { useEffect, useState } from "react";

export type Locale = "zh-CN" | "en-US";

// 语言显示名为 i18n 展示数据（语言名按本土化惯例书写，豁免"代码字符串全英文"规则）。
export const LOCALES: { id: Locale; label: string }[] = [
  { id: "zh-CN", label: "简体中文" },
  { id: "en-US", label: "English" },
];

/** 从系统语言推断默认 Locale。 */
export function systemLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

/** 非 React 环境读取当前语言（如模块顶层注册的回调内部）。 */
export function currentLocale(): Locale {
  const lang = document.documentElement.lang;
  return lang === "zh-CN" || lang === "en-US" ? lang : systemLocale();
}

/** 订阅壳当前语言。 */
export function useLocale(): Locale {
  const [locale, setLocale] = useState<Locale>(() => {
    const lang = document.documentElement.lang;
    if (lang === "zh-CN" || lang === "en-US") return lang;
    return systemLocale();
  });
  useEffect(() => {
    const handler = (e: Event) => setLocale((e as CustomEvent<Locale>).detail);
    window.addEventListener("zannen-locale", handler);
    return () => window.removeEventListener("zannen-locale", handler);
  }, []);
  return locale;
}

/**
 * 生成类型安全的翻译函数。`zh-CN` 为基准字典（缺 key 回退到它）。
 *
 * ```ts
 * const t = createI18n({
 *   "zh-CN": { scan: "扫描设备" },
 *   "en-US": { scan: "Scan devices" },
 * });
 * t(locale, "scan");
 * ```
 */
export function createI18n<T extends Record<string, string>>(
  dicts: Record<Locale, T>,
): (locale: Locale, key: keyof T & string) => string {
  return (locale, key) => dicts[locale]?.[key] ?? dicts["zh-CN"][key] ?? key;
}
