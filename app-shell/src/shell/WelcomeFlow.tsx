/**
 * 首启引导 / 升级欢迎页（精美动效层）。
 *
 * - 全新安装：品牌入场 + 功能亮点分步浮现 + 引导按钮
 * - 升级：版本号滚动过渡 + 跨版本聚合的真实更新日志 + 插件兼容性提示
 * - prefers-reduced-motion 时全部降级为直接呈现
 */

import { Button, GlassCard } from "@zannen/plugin-sdk";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Activity, ArrowDownToLine, Box, PartyPopper, Rocket, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useT, type ShellI18nKey } from "./i18n";
import { compareSemver } from "./updater";
import releaseNotes from "../release-notes.json";

/** 聚合所有晚于 previousVersion 的版本的更新条目（跨版本升级也完整呈现）。 */
function notesSince(previousVersion: string | null | undefined, upTo: string) {
  return releaseNotes.versions.filter(
    (v) =>
      compareSemver(v.version, upTo) <= 0 &&
      (!previousVersion || compareSemver(v.version, previousVersion) > 0),
  );
}

/** 去掉条目里的 markdown 加粗标记。 */
const plain = (s: string) => s.replace(/\*\*/g, "");

export type WelcomeMode = "fresh" | "upgrade";

interface Props {
  mode: WelcomeMode;
  version: string;
  previousVersion?: string | null;
  onDone: () => void;
}

const FEATURES: { icon: typeof Rocket; titleKey: ShellI18nKey; descKey: ShellI18nKey }[] = [
  { icon: Rocket, titleKey: "featureAutoDetectTitle", descKey: "featureAutoDetectDesc" },
  { icon: ArrowDownToLine, titleKey: "featureFlashTitle", descKey: "featureFlashDesc" },
  { icon: Activity, titleKey: "featureDataVizTitle", descKey: "featureDataVizDesc" },
  { icon: Box, titleKey: "feature3dTitle", descKey: "feature3dDesc" },
];

export function WelcomeFlow({ mode, version, previousVersion, onDone }: Props) {
  const reduceMotion = useReducedMotion();
  const [step, setStep] = useState(0);
  const t = useT();

  // 全新安装：功能亮点逐个浮现
  useEffect(() => {
    if (mode !== "fresh" || reduceMotion) {
      setStep(FEATURES.length);
      return;
    }
    if (step >= FEATURES.length) return;
    const timer = setTimeout(() => setStep((s) => s + 1), 340);
    return () => clearTimeout(timer);
  }, [mode, step, reduceMotion]);

  // 确认键快捷键：回车 = 点击主按钮（焦点在可交互元素上时交给原生行为，避免重复触发）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Enter" || e.isComposing) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest("button, input, textarea, select, [contenteditable]")) return;
      e.preventDefault();
      onDone();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDone]);

  // 升级日志聚合一次（供渲染与空态判断共用）
  const upgradeNotes = useMemo(
    () => (mode === "upgrade" ? notesSince(previousVersion, version) : []),
    [mode, previousVersion, version],
  );

  return (
    <div className="welcome-flow">
      <motion.div
        className="welcome-flow-inner"
        initial={reduceMotion ? false : { opacity: 0, scale: 0.96 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
      >
        <motion.div
          className="welcome-logo"
          initial={reduceMotion ? false : { scale: 0.5, rotate: -12, opacity: 0 }}
          animate={{ scale: 1, rotate: 0, opacity: 1 }}
          transition={{ type: "spring", stiffness: 260, damping: 18 }}
        >
          Z
        </motion.div>

        {mode === "fresh" ? (
          <>
            <motion.h1
              initial={reduceMotion ? false : { opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.15 }}
            >
              {t("welcomeTitle")}
            </motion.h1>
            <p className="welcome-sub">{t("welcomeSub")}</p>
            <div className="welcome-features">
              {FEATURES.slice(0, step).map((f) => (
                <motion.div
                  key={f.titleKey}
                  className="welcome-feature zt-glass"
                  initial={{ opacity: 0, x: -18 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ duration: 0.3, ease: "easeOut" }}
                >
                  <f.icon size={17} />
                  <div>
                    <div className="welcome-feature-title">{t(f.titleKey)}</div>
                    <div className="welcome-feature-desc">{t(f.descKey)}</div>
                  </div>
                </motion.div>
              ))}
            </div>
          </>
        ) : (
          <>
            <motion.h1
              initial={reduceMotion ? false : { opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
            >
              <PartyPopper size={22} className="welcome-party" /> {t("upgradeTitle")}
            </motion.h1>
            <div className="welcome-version-track">
              <AnimatePresence mode="popLayout">
                <motion.span
                  key="old"
                  className="welcome-version-old"
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                >
                  v{previousVersion ?? "?"}
                </motion.span>
                <motion.span
                  className="welcome-version-arrow"
                  initial={reduceMotion ? false : { opacity: 0, x: -8 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ delay: 0.25 }}
                >
                  →
                </motion.span>
                <motion.span
                  key="new"
                  className="welcome-version-new"
                  initial={reduceMotion ? false : { opacity: 0, scale: 0.7 }}
                  animate={{ opacity: 1, scale: 1 }}
                  transition={{ delay: 0.4, type: "spring", stiffness: 300, damping: 16 }}
                >
                  v{version}
                </motion.span>
              </AnimatePresence>
            </div>
            <GlassCard className="welcome-notes">
              <div className="welcome-notes-title">
                <Sparkles size={14} /> {t("upgradeNotesTitle")}
              </div>
              <div className="welcome-notes-scroll">
                {upgradeNotes.map((v) =>
                  v.sections.map((sec) => (
                    <div key={`${v.version}-${sec.title}`} className="welcome-notes-section">
                      <div className="welcome-notes-cat">
                        {sec.title} · v{v.version}
                      </div>
                      <ul>
                        {sec.items.map((note, i) => (
                          <li key={i}>{plain(note)}</li>
                        ))}
                      </ul>
                    </div>
                  )),
                )}
                {upgradeNotes.length === 0 && (
                  <ul>
                    <li>{t("upgradeNote1")}</li>
                    <li>{t("upgradeNote2")}</li>
                    <li>{t("upgradeNote3")}</li>
                  </ul>
                )}
              </div>
            </GlassCard>
          </>
        )}

        <motion.div
          initial={reduceMotion ? false : { opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: mode === "fresh" ? 0.2 + FEATURES.length * 0.34 : 0.55 }}
        >
          <Button variant="primary" className="welcome-cta" onClick={onDone}>
            {mode === "fresh" ? t("ctaExplore") : t("ctaEnter")}
          </Button>
        </motion.div>
      </motion.div>
    </div>
  );
}
