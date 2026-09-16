/**
 * 插件宿主视图：按导航选择渲染插件路由，含错误边界与加载态。
 *
 * 插件前端懒装载：选中某插件路由时经 `ensurePluginLoaded` 按需拉取模块
 * （loader 内部缓存进行中的 Promise 防重入）；装载中显示 Spinner，
 * 失败显示 EmptyState + 重试按钮。`shell/settings` 等内建视图不受影响。
 */

import { Component, Suspense, useEffect, type ReactNode } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Button, EmptyState, Spinner } from "@zannen/plugin-sdk";
import { TriangleAlert } from "lucide-react";
import { SHELL_ERR } from "./errors";
import { useT } from "./i18n";
import { LauncherView } from "./LauncherView";
import { ensurePluginLoaded } from "./pluginLoader";
import { SettingsView } from "./SettingsView";
import { useShellStore } from "./store";

interface EBProps {
  children: ReactNode;
  resetKey: string;
  title: string;
}

interface EBState {
  error?: Error;
}

/** 插件视图错误边界：插件崩溃不拖垮壳。 */
class PluginErrorBoundary extends Component<EBProps, EBState> {
  state: EBState = {};

  static getDerivedStateFromError(error: Error): EBState {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error("[plugin-host] render error:", error);
  }

  componentDidUpdate(prev: EBProps) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: undefined });
    }
  }

  render() {
    if (this.state.error) {
      return (
        <EmptyState
          icon={<TriangleAlert size={28} />}
          title={`[${SHELL_ERR.PLUGIN_RENDER_FAILED}] ${this.props.title}`}
          desc={`${this.state.error.message} (code catalog: docs/ERROR-CODES.md)`}
        />
      );
    }
    return this.props.children;
  }
}

export function PluginHost() {
  const plugins = useShellStore((s) => s.plugins);
  const pluginsReady = useShellStore((s) => s.pluginsReady);
  const selection = useShellStore((s) => s.selection);
  const replacePlugin = useShellStore((s) => s.replacePlugin);
  const t = useT();

  const plugin = selection ? plugins.find((p) => p.manifest.id === selection.pluginId) : undefined;
  // 模块未装载且无错误 → 需要（或正在）懒装载
  const needsLoad = !!plugin && !plugin.module && !plugin.error && !!plugin.manifest.frontend;

  useEffect(() => {
    if (pluginsReady && selection && selection.pluginId !== "shell" && needsLoad) {
      void ensurePluginLoaded(selection.pluginId);
    }
  }, [pluginsReady, selection, needsLoad]);

  if (!pluginsReady) {
    return (
      <div className="plugin-host-loading">
        <Spinner size={22} />
        <span>{t("loadingPlugins")}</span>
      </div>
    );
  }

  // —— 计算当前视图内容与统一视图键 ——
  let content: ReactNode;
  if (!selection) {
    content = <LauncherView />;
  } else if (selection.pluginId === "shell") {
    content =
      selection.route === "settings" || selection.route === "updates" ? (
        <SettingsView />
      ) : (
        <EmptyState title={t("unknownBuiltinView")} desc={selection.route} />
      );
  } else if (!plugin) {
    content = <EmptyState title={t("pluginNotFound")} desc={selection.pluginId} />;
  } else if (plugin.error) {
    content = (
      <div className="plugin-host-loading">
        <EmptyState
          icon={<TriangleAlert size={28} />}
          title={t("pluginLoadFailed", { name: plugin.manifest.name })}
          desc={plugin.error}
        />
        <Button variant="ghost" onClick={() => replacePlugin({ manifest: plugin.manifest })}>
          {t("retry")}
        </Button>
      </div>
    );
  } else {
    const View = plugin.module?.routes[selection.route];
    if (!View) {
      // 前端模块仍在懒装载中（无 frontend 的纯后端插件则落入路由缺失分支）
      content =
        !plugin.module && plugin.manifest.frontend ? (
          <div className="plugin-host-loading">
            <Spinner size={22} />
            <span>{t("loadingPlugin", { name: plugin.manifest.name })}</span>
          </div>
        ) : (
          <EmptyState title={t("routeNotImplemented")} desc={`${selection.pluginId}/${selection.route}`} />
        );
    } else {
      const key = `${selection.pluginId}/${selection.route}`;
      content = (
        <PluginErrorBoundary resetKey={key} title={t("viewError")}>
          <Suspense
            fallback={
              <div className="plugin-host-loading">
                <Spinner size={20} />
              </div>
            }
          >
            <View />
          </Suspense>
        </PluginErrorBoundary>
      );
    }
  }

  // 层级/路由切换统一过渡：fade + 轻位移 + 微缩放 + 模糊收放（品牌缓出曲线，
  // 与窗口几何动画同节奏；reduced-motion 由全局 CSS 兜底降级）
  const viewKey = !selection
    ? "home"
    : selection.pluginId === "shell"
      ? "shell/settings"
      : `${selection.pluginId}/${selection.route}`;

  return (
    <AnimatePresence mode="wait">
      <motion.div
        key={viewKey}
        className="plugin-host-view"
        initial={{ opacity: 0, y: 14, scale: 0.986, filter: "blur(6px)" }}
        animate={{ opacity: 1, y: 0, scale: 1, filter: "blur(0px)" }}
        exit={{ opacity: 0, y: -14, scale: 0.986, filter: "blur(6px)" }}
        transition={{ duration: 0.26, ease: [0.22, 1, 0.36, 1] }}
      >
        {content}
      </motion.div>
    </AnimatePresence>
  );
}
