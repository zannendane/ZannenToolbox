/**
 * 插件界面的路由标签栏（单窗口层级导航的子级）：返回上级（模块主页）+ 本插件路由。
 */

import { motion } from "framer-motion";
import { ArrowLeft } from "lucide-react";
import { iconFor } from "./iconMap";
import { useT } from "./i18n";
import { useShellStore } from "./store";

export function PluginTabs({ pluginId }: { pluginId: string }) {
  const plugins = useShellStore((s) => s.plugins);
  const selection = useShellStore((s) => s.selection);
  const select = useShellStore((s) => s.select);
  const goHome = useShellStore((s) => s.goHome);
  const routeTitle = useShellStore((s) => s.routeTitle);
  const t = useT();
  const plugin = plugins.find((p) => p.manifest.id === pluginId);

  if (!plugin) return null;

  return (
    <div className="plugin-tabs">
      <button className="plugin-tab plugin-tab-back" onClick={goHome} title={t("backToModules")}>
        <ArrowLeft size={14} />
        <span>{t("backToModules")}</span>
      </button>
      <span className="plugin-tabs-divider" />
      {plugin.manifest.routes.map((route) => {
        const active = selection?.route === route.path;
        const Icon = iconFor(route.icon);
        return (
          <button
            key={route.path}
            className={`plugin-tab${active ? " is-active" : ""}`}
            onClick={() => select({ pluginId, route: route.path })}
          >
            {active && (
              <motion.span
                className="plugin-tab-pill"
                layoutId={`plugin-tab-${pluginId}`}
                transition={{ type: "spring", stiffness: 500, damping: 36 }}
              />
            )}
            <Icon size={14} />
            <span>{routeTitle(pluginId, route.path, route.title)}</span>
          </button>
        );
      })}
    </div>
  );
}
