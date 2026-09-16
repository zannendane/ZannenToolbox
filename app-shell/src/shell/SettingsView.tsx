/**
 * 壳设置视图：外观/语言快捷设置 + 更新中心（壳级功能的家）。
 * 从任意层级进入，返回时还原到进入前的位置（store.settingsFrom）。
 */

import { Button, GlassCard, LOCALES, SectionTitle, Segmented, Toggle } from "@zannen/plugin-sdk";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { ArrowLeft, Languages, Palette, Power } from "lucide-react";
import { useEffect, useState } from "react";
import { useT } from "./i18n";
import { useLocaleStore } from "./locale";
import { useShellStore } from "./store";
import { useThemeStore, type ThemeMode } from "./theme";
import { UpdateCenterView } from "./UpdateCenterView";

const THEME_OPTIONS: { value: ThemeMode; labelKey: "themeAuto" | "themeLight" | "themeDark" }[] = [
  { value: "auto", labelKey: "themeAuto" },
  { value: "light", labelKey: "themeLight" },
  { value: "dark", labelKey: "themeDark" },
];

export function SettingsView() {
  const t = useT();
  const closeSettings = useShellStore((s) => s.closeSettings);
  const themeMode = useThemeStore((s) => s.mode);
  const setThemeMode = useThemeStore((s) => s.setMode);
  const locale = useLocaleStore((s) => s.locale);
  const setLocale = useLocaleStore((s) => s.setLocale);
  const [autostart, setAutostart] = useState(false);

  // 读取当前自启动状态（默认关闭；开关状态由系统注册表/登录项如实反映）
  useEffect(() => {
    void isEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(false));
  }, []);

  const applyAutostart = async (enabled: boolean) => {
    setAutostart(enabled);
    try {
      if (enabled) {
        await enable();
      } else {
        await disable();
      }
    } catch (e) {
      setAutostart(!enabled);
      console.error("[settings] autostart toggle failed:", e);
    }
  };

  return (
    <div className="zd-view">
      <div className="zd-toolbar">
        <Button variant="ghost" onClick={closeSettings}>
          <ArrowLeft size={14} /> {t("settingsBack")}
        </Button>
      </div>
      <SectionTitle title={t("settingsTitle")} desc={t("settingsDesc")} />

      <GlassCard>
        <div className="settings-card-title">
          <Palette size={15} /> {t("appearanceSection")}
        </div>
        <div className="settings-row">
          <span className="settings-label">{t("theme")}</span>
          <Segmented
            options={THEME_OPTIONS.map((o) => ({ value: o.value, label: t(o.labelKey) }))}
            value={themeMode}
            onChange={setThemeMode}
          />
        </div>
        <div className="settings-row">
          <span className="settings-label">
            <Languages size={13} style={{ verticalAlign: -2, marginRight: 4 }} />
            {t("language")}
          </span>
          <Segmented
            options={LOCALES.map((l) => ({ value: l.id, label: l.label }))}
            value={locale}
            onChange={setLocale}
          />
        </div>
      </GlassCard>

      {/* 系统：开机自启动（默认关闭；macOS 与 Windows 均无需系统授权弹窗） */}
      <GlassCard>
        <div className="settings-card-title">
          <Power size={15} /> {t("autostartTitle")}
        </div>
        <div className="settings-row">
          <span className="settings-label settings-label-wrap">{t("autostartDesc")}</span>
          <Toggle
            checked={autostart}
            onChange={(v) => void applyAutostart(v)}
            label={t("autostartTitle")}
          />
        </div>
      </GlassCard>

      <UpdateCenterView />
    </div>
  );
}
