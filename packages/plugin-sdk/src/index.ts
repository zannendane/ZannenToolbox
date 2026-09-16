/**
 * @zannen/plugin-sdk — 插件前端 SDK。
 *
 * 由壳在运行时经 vendor ESM 提供给插件（import specifier 重写），
 * 保证插件与壳共享同一份 React / Tauri API 实例。
 */

export { invokePlugin, invokeShell, onBus, registerPluginTitles, setPluginTitles, type BusEnvelope, type PluginLocalizedTitles, type PluginTitlesProvider } from "./ipc";
export { definePlugin, type PluginModule, type PluginManifestDto } from "./types";
export { useBusEvent, useTheme, type ResolvedTheme } from "./hooks";
export { createI18n, currentLocale, systemLocale, useLocale, LOCALES, type Locale } from "./i18n";
export {
  Badge,
  Button,
  EmptyState,
  GlassCard,
  Progress,
  SectionTitle,
  Segmented,
  Spinner,
  Toggle,
} from "./components";
