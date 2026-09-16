/**
 * 壳应用：插件装载、事件总线桥接、首启识别（全新/升级）、静默更新检查。
 */

import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { animateWindowResize } from "./shell/windowAnim";
import { invokeShell, type PluginManifestDto } from "@zannen/plugin-sdk";
import { version as fallbackVersion } from "../package.json";
import { initBusBridge } from "./shell/bus";
import { HOME_WINDOW, PLUGIN_WINDOW } from "./shell/layout";
import { initTitleBridge } from "./shell/store";
import { ensurePluginLoaded, preloadPluginAssets } from "./shell/pluginLoader";
import { FloatingBar } from "./shell/FloatingBar";
import { OverlayHost } from "./shell/OverlayHost";
import { PluginHost } from "./shell/PluginHost";
import { PluginTabs } from "./shell/PluginTabs";
import { TitleBar } from "./shell/TitleBar";
import { TopBar } from "./shell/TopBar";
import { useShellStore } from "./shell/store";
import { WelcomeFlow } from "./shell/WelcomeFlow";
import { checkAppUpdate, loadUpdateSources } from "./shell/updater";

interface ShellInfoDto {
  version: string;
  target: string;
  plugins: string[];
}

type WelcomeState = { mode: "fresh" | "upgrade"; previous: string | null } | null;

/**
 * 悬浮窗模式：窗口 URL 带 `?overlay=<pluginId>`（壳命令 overlay_open 创建）时，
 * 整窗只渲染该插件的 overlay 组件（OverlayHost），透明背景无壳 chrome。
 * 模块顶层判定 + 打标（首帧渲染前生效，配合 base.css 的 .zt-overlay-mode）。
 */
const overlayPluginId = new URLSearchParams(window.location.search).get("overlay");
if (overlayPluginId) {
  document.documentElement.classList.add("zt-overlay-mode");
}

export function App() {
  const setPlugins = useShellStore((s) => s.setPlugins);
  const [welcome, setWelcome] = useState<WelcomeState>(null);
  // shell_info 返回前以包版本兜底（不再硬编码，随 package.json 同步）。
  const [version, setVersion] = useState(fallbackVersion);

  // 窗口尺寸跟随层级：主菜单窄竖，插件/设置扩为工作区（尺寸常量见 shell/layout.ts）。
  // 扩缩保持窗口中心点不动（读当前中心 → 变尺寸 → 反推位置），避免跳回屏幕中心。
  // 仅在"主页↔子级"边界翻转时调整，不打断用户在子级内的自定义尺寸。
  const selection = useShellStore((s) => s.selection);
  const wasHomeRef = useRef<boolean | null>(null);
  useEffect(() => {
    if (overlayPluginId) return; // 悬浮窗尺寸由用户/壳命令管理，不随层级动画
    const isHome = selection === null;
    if (wasHomeRef.current === isHome) return;
    wasHomeRef.current = isHome;
    const win = getCurrentWindow();
    const { width: w, height: h } = isHome ? HOME_WINDOW : PLUGIN_WINDOW;
    void animateWindowResize(win, w, h).catch(() => {});
  }, [selection]);

  useEffect(() => {
    let disposed = false;

    (async () => {
      // 启动链路并行化：总线桥接 / 壳信息与首启识别 / 插件清单 三路并发，
      // 首帧渲染不等待任何一路。
      initTitleBridge();
      const busReady = initBusBridge();

      const infoReady = (async () => {
        // 悬浮窗模式跳过首启/升级识别（欢迎页只属主窗口）
        if (overlayPluginId) return;
        try {
          const info = await invokeShell<ShellInfoDto>("shell_info");
          if (disposed) return;
          setVersion(info.version);
          console.info(`[shell] ZannenToolbox v${info.version} (${info.target})`);
          const state = await invokeShell<{ last_run_version?: string } | null>("state_read");
          if (disposed) return;
          const last = state?.last_run_version;
          if (!last) {
            setWelcome({ mode: "fresh", previous: null });
          } else if (last !== info.version) {
            setWelcome({ mode: "upgrade", previous: last });
          }
        } catch (e) {
          console.warn("[shell] failed to read first-run state:", e);
        }
      })();

      const pluginsReady = (async () => {
        // 插件注册：仅登记清单，前端模块由 PluginHost 选中路由时懒装载
        try {
          const manifests = await invokeShell<PluginManifestDto[]>("plugin_list");
          if (disposed) return;
          setPlugins(manifests.map((manifest) => ({ manifest })));

          // 空闲预热：注册完成后后台装载全部插件前端（当前选中项优先），
          // 首次点击即达；requestIdleCallback 在老 WebView 上退化为定时器。
          const plugins = useShellStore.getState().plugins;
          const selectedId = useShellStore.getState().selection?.pluginId;
          const idle: (cb: () => void) => void =
            "requestIdleCallback" in window
              ? (cb) => window.requestIdleCallback(cb, { timeout: 1500 })
              : (cb) => setTimeout(cb, 400);
          const ordered = [...plugins].sort((a, b) =>
            a.manifest.id === selectedId ? -1 : b.manifest.id === selectedId ? 1 : 0,
          );
          for (const p of ordered) {
            if (p.manifest.frontend) {
              preloadPluginAssets(p.manifest); // CSS 立即注入，避免样式闪烁
              idle(() => void ensurePluginLoaded(p.manifest.id));
            }
          }
        } catch (e) {
          console.error("[shell] failed to list plugins:", e);
          if (!disposed) setPlugins([]);
        }
      })();

      // 静默本体更新检查（延迟 3s，未配置源/离线自动跳过；悬浮窗模式跳过）
      void (async () => {
        if (overlayPluginId) return;
        const sources = await loadUpdateSources();
        setTimeout(async () => {
          const update = await checkAppUpdate(sources);
          if (update && !disposed) {
            useShellStore.getState().setAppUpdateAvailable(update.version);
          }
        }, 3000);
      })();

      // 三路并发汇合（均自带容错，不会相互阻塞）
      await Promise.allSettled([busReady, infoReady, pluginsReady]);
    })();

    return () => {
      disposed = true;
    };
  }, [setPlugins]);

  const closeWelcome = async () => {
    setWelcome(null);
    try {
      await invokeShell("state_write", {
        state: { last_run_version: version, welcomed_at: new Date().toISOString() },
      });
    } catch (e) {
      console.warn("[shell] failed to write state:", e);
    }
  };

  // 悬浮窗模式：整窗仅渲染插件 overlay 组件（boot 逻辑与正常壳共用，见上方 effect）
  if (overlayPluginId) {
    return <OverlayHost pluginId={overlayPluginId} />;
  }

  return (
    <div className="app-frame">
      <TitleBar />
      <div className="app-body">
        <div className="app-main">
          <TopBar />
          {/* 插件子级界面：路由标签栏（含返回上级「模块」），高度展开过渡 */}
          <AnimatePresence initial={false}>
            {selection && selection.pluginId !== "shell" && (
              <motion.div
                key="plugin-tabs-wrap"
                style={{ overflow: "hidden" }}
                initial={{ height: 0, opacity: 0 }}
                animate={{ height: "auto", opacity: 1 }}
                exit={{ height: 0, opacity: 0 }}
                transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
              >
                <PluginTabs pluginId={selection.pluginId} />
              </motion.div>
            )}
          </AnimatePresence>
          <main className="app-content">
            <PluginHost />
          </main>
        </div>
      </div>
      {/* 左下悬浮工具栈：全层级常驻 */}
      <FloatingBar />
      <AnimatePresence>
        {welcome && (
          <WelcomeFlow
            mode={welcome.mode}
            version={version}
            previousVersion={welcome.previous}
            onDone={() => void closeWelcome()}
          />
        )}
      </AnimatePresence>
    </div>
  );
}
