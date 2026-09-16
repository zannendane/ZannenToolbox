/**
 * 语言系统：zh-CN / en-US。
 *
 * 与主题模块同构：
 * - 当前语言写入 `document.documentElement.lang`，并派发 `zannen-locale`
 *   自定义事件——插件经 SDK 的 `useLocale()` 订阅（同一文档，天然同步）。
 * - 选择持久化到 localStorage；无历史选择时跟随系统语言（systemLocale）。
 */

import { systemLocale, type Locale } from "@zannen/plugin-sdk";
import { create } from "zustand";

const STORAGE_KEY = "zannen-locale";

interface LocaleState {
  locale: Locale;
  setLocale: (locale: Locale) => void;
}

function readInitialLocale(): Locale {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "zh-CN" || v === "en-US" ? v : systemLocale();
}

/** 应用语言：<html lang> + 事件广播。 */
function applyLocale(locale: Locale) {
  document.documentElement.lang = locale;
  window.dispatchEvent(new CustomEvent<Locale>("zannen-locale", { detail: locale }));
}

export const useLocaleStore = create<LocaleState>((set) => ({
  locale: readInitialLocale(),
  setLocale: (locale) => {
    localStorage.setItem(STORAGE_KEY, locale);
    set({ locale });
    applyLocale(locale);
  },
}));

/** 启动初始化：把当前语言写到 <html lang>（须在首次渲染前调用），并挂上跨窗口同步。 */
export function initLocale(): void {
  applyLocale(useLocaleStore.getState().locale);
  window.addEventListener("storage", (e) => {
    if (e.key !== STORAGE_KEY) return;
    const v = e.newValue;
    if ((v === "zh-CN" || v === "en-US") && v !== useLocaleStore.getState().locale) {
      useLocaleStore.getState().setLocale(v);
    }
  });
}
