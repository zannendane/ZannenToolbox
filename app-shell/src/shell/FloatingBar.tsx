/**
 * 左下角悬浮工具栈：外观切换 / 语言切换 / 壳设置入口。
 * 全层级常驻（主菜单、插件子级、设置视图一致）；与内容区无遮挡（内容区底部留白）。
 */

import { AnimatePresence, motion } from "framer-motion";
import { Globe, Monitor, Moon, Settings, Sun } from "lucide-react";
import { LOCALES, type Locale } from "@zannen/plugin-sdk";
import { useT } from "./i18n";
import { useLocaleStore } from "./locale";
import { useShellStore } from "./store";
import { useThemeStore, type ThemeMode } from "./theme";

const THEME_CYCLE: Record<ThemeMode, ThemeMode> = { auto: "light", light: "dark", dark: "auto" };
const THEME_ICON: Record<ThemeMode, typeof Sun> = { auto: Monitor, light: Sun, dark: Moon };
const LOCALE_CYCLE: Record<Locale, Locale> = { "zh-CN": "en-US", "en-US": "zh-CN" };

export function FloatingBar() {
  const t = useT();
  const themeMode = useThemeStore((s) => s.mode);
  const setThemeMode = useThemeStore((s) => s.setMode);
  const locale = useLocaleStore((s) => s.locale);
  const setLocale = useLocaleStore((s) => s.setLocale);
  const openSettings = useShellStore((s) => s.openSettings);
  const selection = useShellStore((s) => s.selection);
  const appUpdateAvailable = useShellStore((s) => s.appUpdateAvailable);
  const inSettings = selection?.pluginId === "shell" && selection.route === "settings";

  const ThemeIcon = THEME_ICON[themeMode];
  const themeLabelKey = themeMode === "auto" ? "themeAuto" : themeMode === "light" ? "themeLight" : "themeDark";

  return (
    <div className="floatbar">
      <motion.button
        className="floatbar-btn"
        title={`${t("theme")}: ${t(themeLabelKey)}`}
        onClick={() => setThemeMode(THEME_CYCLE[themeMode])}
        whileTap={{ scale: 0.88 }}
      >
        <AnimatePresence mode="wait" initial={false}>
          <motion.span
            key={themeMode}
            initial={{ rotate: -60, opacity: 0 }}
            animate={{ rotate: 0, opacity: 1 }}
            exit={{ rotate: 60, opacity: 0 }}
            transition={{ duration: 0.18 }}
            style={{ display: "inline-flex" }}
          >
            <ThemeIcon size={16} />
          </motion.span>
        </AnimatePresence>
      </motion.button>

      <motion.button
        className="floatbar-btn"
        title={`${t("language")}: ${LOCALES.find((l) => l.id === locale)?.label ?? locale}`}
        onClick={() => setLocale(LOCALE_CYCLE[locale])}
        whileTap={{ scale: 0.88 }}
      >
        <Globe size={16} />
      </motion.button>

      {!inSettings && (
        <motion.button
          className="floatbar-btn"
          title={t("settingsTitle")}
          onClick={openSettings}
          whileTap={{ scale: 0.88 }}
          initial={{ opacity: 0, scale: 0.8 }}
          animate={{ opacity: 1, scale: 1 }}
        >
          <Settings size={16} />
          {appUpdateAvailable && <span className="floatbar-dot" />}
        </motion.button>
      )}
    </div>
  );
}
